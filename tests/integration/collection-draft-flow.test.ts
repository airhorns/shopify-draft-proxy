import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

describe('collection draft flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages collectionCreate locally and exposes the new collection through collection and collections reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateCollection($input: CollectionInput!) { collectionCreate(input: $input) { collection { id title handle products(first: 10) { nodes { id title } pageInfo { hasNextPage hasPreviousPage } } } userErrors { field message } } }',
        variables: {
          input: {
            title: 'Winter Hats',
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.collectionCreate.userErrors).toEqual([]);
    const createdCollection = createResponse.body.data.collectionCreate.collection as {
      id: string;
      title: string;
      handle: string;
      products: { nodes: unknown[]; pageInfo: { hasNextPage: boolean; hasPreviousPage: boolean } };
    };
    expect(createdCollection).toEqual({
      id: expect.stringMatching(/^gid:\/\/shopify\/Collection\/\d+$/),
      title: 'Winter Hats',
      handle: 'winter-hats',
      products: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ReadCollection($id: ID!) { collection(id: $id) { id title handle products(first: 10) { nodes { id title } pageInfo { hasNextPage hasPreviousPage } } } collections(first: 10) { nodes { id title handle products(first: 10) { nodes { id } pageInfo { hasNextPage hasPreviousPage } } } } }',
        variables: {
          id: createdCollection.id,
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        collection: createdCollection,
        collections: {
          nodes: [createdCollection],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages rich collectionCreate and collectionUpdate fields for downstream reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateRichCollection($input: CollectionInput!, $productId: ID!) { collectionCreate(input: $input) { collection { id legacyResourceId title handle updatedAt description descriptionHtml image { url altText width height } productsCount { count precision } hasProduct(id: $productId) sortOrder templateSuffix seo { title description } ruleSet { appliedDisjunctively rules { column relation condition conditionObject } } } userErrors { field message } } }',
        variables: {
          productId: 'gid://shopify/Product/404',
          input: {
            title: 'Rich Winter Hats',
            handle: 'rich-winter-hats',
            descriptionHtml: '<p>Warm <strong>winter</strong> hats</p>',
            image: {
              src: 'https://cdn.shopify.com/s/files/1/0000/0001/collections/winter-hats.jpg',
              altText: 'Winter hats',
              width: 1200,
              height: 800,
            },
            sortOrder: 'ALPHA_ASC',
            templateSuffix: 'seasonal',
            seo: {
              title: 'Winter Hat SEO',
              description: 'Warm winter hats',
            },
            ruleSet: {
              appliedDisjunctively: true,
              rules: [
                {
                  column: 'TAG',
                  relation: 'EQUALS',
                  condition: 'winter',
                },
              ],
            },
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.collectionCreate.userErrors).toEqual([]);
    const createdCollection = createResponse.body.data.collectionCreate.collection;
    expect(createdCollection).toEqual({
      id: expect.stringMatching(/^gid:\/\/shopify\/Collection\/\d+$/),
      legacyResourceId: expect.stringMatching(/^\d+$/),
      title: 'Rich Winter Hats',
      handle: 'rich-winter-hats',
      updatedAt: expect.stringMatching(/^\d{4}-\d{2}-\d{2}T/),
      description: 'Warm winter hats',
      descriptionHtml: '<p>Warm <strong>winter</strong> hats</p>',
      image: {
        url: 'https://cdn.shopify.com/s/files/1/0000/0001/collections/winter-hats.jpg',
        altText: 'Winter hats',
        width: 1200,
        height: 800,
      },
      productsCount: {
        count: 0,
        precision: 'EXACT',
      },
      hasProduct: false,
      sortOrder: 'ALPHA_ASC',
      templateSuffix: 'seasonal',
      seo: {
        title: 'Winter Hat SEO',
        description: 'Warm winter hats',
      },
      ruleSet: {
        appliedDisjunctively: true,
        rules: [
          {
            column: 'TAG',
            relation: 'EQUALS',
            condition: 'winter',
            conditionObject: null,
          },
        ],
      },
    });

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateRichCollection($input: CollectionInput!) { collectionUpdate(input: $input) { collection { id title handle description descriptionHtml sortOrder templateSuffix seo { title description } ruleSet { appliedDisjunctively rules { column relation condition } } } userErrors { field message } } }',
        variables: {
          input: {
            id: createdCollection.id,
            title: 'Rich Winter Hats Draft',
            descriptionHtml: '<p>Updated winter hats</p>',
            sortOrder: 'MANUAL',
            redirectNewHandle: true,
            seo: {
              title: 'Updated Winter SEO',
            },
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body).toEqual({
      data: {
        collectionUpdate: {
          collection: {
            id: createdCollection.id,
            title: 'Rich Winter Hats Draft',
            handle: 'rich-winter-hats',
            description: 'Updated winter hats',
            descriptionHtml: '<p>Updated winter hats</p>',
            sortOrder: 'MANUAL',
            templateSuffix: 'seasonal',
            seo: {
              title: 'Updated Winter SEO',
              description: 'Warm winter hats',
            },
            ruleSet: {
              appliedDisjunctively: true,
              rules: [
                {
                  column: 'TAG',
                  relation: 'EQUALS',
                  condition: 'winter',
                },
              ],
            },
          },
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ReadRichCollection($id: ID!) { collection(id: $id) { id title handle descriptionHtml sortOrder templateSuffix seo { title description } ruleSet { appliedDisjunctively rules { column relation condition } } } collections(first: 10) { nodes { id title handle descriptionHtml sortOrder templateSuffix seo { title description } } } }',
        variables: {
          id: createdCollection.id,
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        collection: {
          id: createdCollection.id,
          title: 'Rich Winter Hats Draft',
          handle: 'rich-winter-hats',
          descriptionHtml: '<p>Updated winter hats</p>',
          sortOrder: 'MANUAL',
          templateSuffix: 'seasonal',
          seo: {
            title: 'Updated Winter SEO',
            description: 'Warm winter hats',
          },
          ruleSet: {
            appliedDisjunctively: true,
            rules: [
              {
                column: 'TAG',
                relation: 'EQUALS',
                condition: 'winter',
              },
            ],
          },
        },
        collections: {
          nodes: [
            {
              id: createdCollection.id,
              title: 'Rich Winter Hats Draft',
              handle: 'rich-winter-hats',
              descriptionHtml: '<p>Updated winter hats</p>',
              sortOrder: 'MANUAL',
              templateSuffix: 'seasonal',
              seo: {
                title: 'Updated Winter SEO',
                description: 'Warm winter hats',
              },
            },
          ],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages collectionUpdate locally and refreshes both top-level and nested product collection reads', async () => {
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/10',
        legacyResourceId: '10',
        title: 'Hydrated Hat',
        handle: 'hydrated-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: ['hat'],
        totalInventory: 8,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/10', [
      {
        id: 'gid://shopify/Collection/900',
        productId: 'gid://shopify/Product/10',
        title: 'Hydrated Collection',
        handle: 'hydrated-collection',
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp(config).callback();

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateCollection($input: CollectionInput!) { collectionUpdate(input: $input) { collection { id title handle products(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } userErrors { field message } } }',
        variables: {
          input: {
            id: 'gid://shopify/Collection/900',
            title: 'Hydrated Collection Draft',
            handle: 'hydrated-collection-draft',
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body).toEqual({
      data: {
        collectionUpdate: {
          collection: {
            id: 'gid://shopify/Collection/900',
            title: 'Hydrated Collection Draft',
            handle: 'hydrated-collection-draft',
            products: {
              nodes: [
                {
                  id: 'gid://shopify/Product/10',
                  title: 'Hydrated Hat',
                  handle: 'hydrated-hat',
                },
              ],
              pageInfo: {
                hasNextPage: false,
                hasPreviousPage: false,
              },
            },
          },
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query UpdatedCollection($collectionId: ID!, $productId: ID!) { collection(id: $collectionId) { id title handle products(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } product(id: $productId) { id collections(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: {
          collectionId: 'gid://shopify/Collection/900',
          productId: 'gid://shopify/Product/10',
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        collection: {
          id: 'gid://shopify/Collection/900',
          title: 'Hydrated Collection Draft',
          handle: 'hydrated-collection-draft',
          products: {
            nodes: [
              {
                id: 'gid://shopify/Product/10',
                title: 'Hydrated Hat',
                handle: 'hydrated-hat',
              },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
        product: {
          id: 'gid://shopify/Product/10',
          collections: {
            nodes: [
              {
                id: 'gid://shopify/Collection/900',
                title: 'Hydrated Collection Draft',
                handle: 'hydrated-collection-draft',
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
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages collectionDelete locally and removes the collection from both top-level and nested product reads', async () => {
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/20',
        legacyResourceId: '20',
        title: 'Delete Me Hat',
        handle: 'delete-me-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: [],
        totalInventory: 2,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/20', [
      {
        id: 'gid://shopify/Collection/901',
        productId: 'gid://shopify/Product/20',
        title: 'Delete Me Collection',
        handle: 'delete-me-collection',
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp(config).callback();

    const deleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeleteCollection($input: CollectionDeleteInput!) { collectionDelete(input: $input) { deletedCollectionId userErrors { field message } } }',
        variables: {
          input: {
            id: 'gid://shopify/Collection/901',
          },
        },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body).toEqual({
      data: {
        collectionDelete: {
          deletedCollectionId: 'gid://shopify/Collection/901',
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query DeletedCollection($collectionId: ID!, $productId: ID!) { collection(id: $collectionId) { id title handle } collections(first: 10) { nodes { id title handle } } product(id: $productId) { id collections(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: {
          collectionId: 'gid://shopify/Collection/901',
          productId: 'gid://shopify/Product/20',
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        collection: null,
        collections: {
          nodes: [],
        },
        product: {
          id: 'gid://shopify/Product/20',
          collections: {
            nodes: [],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages collectionAddProducts locally with downstream collection and product membership visibility', async () => {
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/30',
        legacyResourceId: '30',
        title: 'Green Hat',
        handle: 'green-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: ['green'],
        totalInventory: 5,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
      {
        id: 'gid://shopify/Product/31',
        legacyResourceId: '31',
        title: 'Blue Hat',
        handle: 'blue-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: ['blue'],
        totalInventory: 7,
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
        id: 'gid://shopify/Collection/930',
        title: 'Featured Hats',
        handle: 'featured-hats',
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp(config).callback();

    const addResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation AddProducts($id: ID!, $productIds: [ID!]!) { collectionAddProducts(id: $id, productIds: $productIds) { collection { id title handle products(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } userErrors { field message } } }',
        variables: {
          id: 'gid://shopify/Collection/930',
          productIds: ['gid://shopify/Product/30', 'gid://shopify/Product/31'],
        },
      });

    expect(addResponse.status).toBe(200);
    expect(addResponse.body).toEqual({
      data: {
        collectionAddProducts: {
          collection: {
            id: 'gid://shopify/Collection/930',
            title: 'Featured Hats',
            handle: 'featured-hats',
            products: {
              nodes: [
                { id: 'gid://shopify/Product/31', title: 'Blue Hat', handle: 'blue-hat' },
                { id: 'gid://shopify/Product/30', title: 'Green Hat', handle: 'green-hat' },
              ],
              pageInfo: {
                hasNextPage: false,
                hasPreviousPage: false,
              },
            },
          },
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query AddedProducts($collectionId: ID!, $firstProductId: ID!, $secondProductId: ID!) { collection(id: $collectionId) { id title handle products(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } first: product(id: $firstProductId) { id collections(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } second: product(id: $secondProductId) { id collections(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: {
          collectionId: 'gid://shopify/Collection/930',
          firstProductId: 'gid://shopify/Product/30',
          secondProductId: 'gid://shopify/Product/31',
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        collection: {
          id: 'gid://shopify/Collection/930',
          title: 'Featured Hats',
          handle: 'featured-hats',
          products: {
            nodes: [
              { id: 'gid://shopify/Product/31', title: 'Blue Hat', handle: 'blue-hat' },
              { id: 'gid://shopify/Product/30', title: 'Green Hat', handle: 'green-hat' },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
        first: {
          id: 'gid://shopify/Product/30',
          collections: {
            nodes: [{ id: 'gid://shopify/Collection/930', title: 'Featured Hats', handle: 'featured-hats' }],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
        second: {
          id: 'gid://shopify/Product/31',
          collections: {
            nodes: [{ id: 'gid://shopify/Collection/930', title: 'Featured Hats', handle: 'featured-hats' }],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('keeps collectionAddProducts atomic when any product is already a member', async () => {
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/40',
        legacyResourceId: '40',
        title: 'Existing Member Hat',
        handle: 'existing-member-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: [],
        totalInventory: 3,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
      {
        id: 'gid://shopify/Product/41',
        legacyResourceId: '41',
        title: 'New Member Hat',
        handle: 'new-member-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: [],
        totalInventory: 4,
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
        id: 'gid://shopify/Collection/940',
        title: 'Atomic Hats',
        handle: 'atomic-hats',
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/40', [
      {
        id: 'gid://shopify/Collection/940',
        productId: 'gid://shopify/Product/40',
        title: 'Atomic Hats',
        handle: 'atomic-hats',
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp(config).callback();

    const addResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation AddProducts($id: ID!, $productIds: [ID!]!) { collectionAddProducts(id: $id, productIds: $productIds) { collection { id title products(first: 10) { nodes { id } } } userErrors { field message } } }',
        variables: {
          id: 'gid://shopify/Collection/940',
          productIds: ['gid://shopify/Product/40', 'gid://shopify/Product/41'],
        },
      });

    expect(addResponse.status).toBe(200);
    expect(addResponse.body).toEqual({
      data: {
        collectionAddProducts: {
          collection: null,
          userErrors: [{ field: ['productIds'], message: 'Product is already in the collection' }],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query AtomicMembership($collectionId: ID!, $existingProductId: ID!, $newProductId: ID!) { collection(id: $collectionId) { id products(first: 10) { nodes { id } pageInfo { hasNextPage hasPreviousPage } } } existing: product(id: $existingProductId) { id collections(first: 10) { nodes { id } pageInfo { hasNextPage hasPreviousPage } } } added: product(id: $newProductId) { id collections(first: 10) { nodes { id } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: {
          collectionId: 'gid://shopify/Collection/940',
          existingProductId: 'gid://shopify/Product/40',
          newProductId: 'gid://shopify/Product/41',
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        collection: {
          id: 'gid://shopify/Collection/940',
          products: {
            nodes: [{ id: 'gid://shopify/Product/40' }],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
        existing: {
          id: 'gid://shopify/Product/40',
          collections: {
            nodes: [{ id: 'gid://shopify/Collection/940' }],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
        added: {
          id: 'gid://shopify/Product/41',
          collections: {
            nodes: [],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages collectionRemoveProducts locally with an async-shaped job and downstream membership removal', async () => {
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/50',
        legacyResourceId: '50',
        title: 'Red Hat',
        handle: 'red-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: [],
        totalInventory: 6,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
      {
        id: 'gid://shopify/Product/51',
        legacyResourceId: '51',
        title: 'Yellow Hat',
        handle: 'yellow-hat',
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
    ]);
    store.upsertBaseCollections([
      {
        id: 'gid://shopify/Collection/950',
        title: 'Remove Hats',
        handle: 'remove-hats',
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/50', [
      {
        id: 'gid://shopify/Collection/950',
        productId: 'gid://shopify/Product/50',
        title: 'Remove Hats',
        handle: 'remove-hats',
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/51', [
      {
        id: 'gid://shopify/Collection/950',
        productId: 'gid://shopify/Product/51',
        title: 'Remove Hats',
        handle: 'remove-hats',
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp(config).callback();

    const removeResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation RemoveProducts($id: ID!, $productIds: [ID!]!) { collectionRemoveProducts(id: $id, productIds: $productIds) { job { id done } userErrors { field message } } }',
        variables: {
          id: 'gid://shopify/Collection/950',
          productIds: ['gid://shopify/Product/50', 'gid://shopify/Product/999999'],
        },
      });

    expect(removeResponse.status).toBe(200);
    expect(removeResponse.body).toEqual({
      data: {
        collectionRemoveProducts: {
          job: {
            id: expect.stringMatching(/^gid:\/\/shopify\/Job\/.+$/),
            done: false,
          },
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query RemovedProducts($collectionId: ID!, $removedProductId: ID!, $untouchedProductId: ID!) { collection(id: $collectionId) { id products(first: 10) { nodes { id title } pageInfo { hasNextPage hasPreviousPage } } } removed: product(id: $removedProductId) { id collections(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } untouched: product(id: $untouchedProductId) { id collections(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: {
          collectionId: 'gid://shopify/Collection/950',
          removedProductId: 'gid://shopify/Product/50',
          untouchedProductId: 'gid://shopify/Product/51',
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        collection: {
          id: 'gid://shopify/Collection/950',
          products: {
            nodes: [{ id: 'gid://shopify/Product/51', title: 'Yellow Hat' }],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
        removed: {
          id: 'gid://shopify/Product/50',
          collections: {
            nodes: [],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
        untouched: {
          id: 'gid://shopify/Product/51',
          collections: {
            nodes: [{ id: 'gid://shopify/Collection/950', title: 'Remove Hats', handle: 'remove-hats' }],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
