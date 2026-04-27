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

function makeBaseProduct(id: string, title: string, handle: string) {
  const legacyResourceId = id.split('/').at(-1) ?? id;
  return {
    id,
    legacyResourceId,
    title,
    handle,
    status: 'ACTIVE' as const,
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

  it('stages metafields for collection owners and exposes set and delete effects through collection reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation CreateCollection($input: CollectionInput!) { collectionCreate(input: $input) { collection { id title handle metafield(namespace: "custom", key: "season") { id } metafields(first: 10) { nodes { id } pageInfo { hasNextPage hasPreviousPage } } } userErrors { field message } } }',
        variables: {
          input: {
            title: 'Collection Metafield Hats',
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.collectionCreate.userErrors).toEqual([]);
    const collection = createResponse.body.data.collectionCreate.collection as {
      id: string;
      metafield: unknown;
      metafields: { nodes: unknown[]; pageInfo: { hasNextPage: boolean; hasPreviousPage: boolean } };
    };
    expect(collection.metafield).toBeNull();
    expect(collection.metafields).toEqual({
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
      },
    });

    const setResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation SetCollectionMetafield($metafields: [MetafieldsSetInput!]!) { metafieldsSet(metafields: $metafields) { metafields { id namespace key type value ownerType compareDigest } userErrors { field message code elementIndex } } }',
        variables: {
          metafields: [
            {
              ownerId: collection.id,
              namespace: 'custom',
              key: 'season',
              type: 'single_line_text_field',
              value: 'Winter',
            },
          ],
        },
      });

    expect(setResponse.status).toBe(200);
    expect(setResponse.body.data.metafieldsSet.userErrors).toEqual([]);
    expect(setResponse.body.data.metafieldsSet.metafields).toEqual([
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/Metafield\//),
        namespace: 'custom',
        key: 'season',
        type: 'single_line_text_field',
        value: 'Winter',
        ownerType: 'COLLECTION',
        compareDigest: expect.stringMatching(/^draft:/),
      },
    ]);

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'query CollectionMetafields($id: ID!) { collection(id: $id) { id season: metafield(namespace: "custom", key: "season") { id namespace key value ownerType } metafields(first: 10) { nodes { id namespace key value ownerType } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: {
          id: collection.id,
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.collection.season).toMatchObject({
      namespace: 'custom',
      key: 'season',
      value: 'Winter',
      ownerType: 'COLLECTION',
    });
    expect(readResponse.body.data.collection.metafields.nodes).toEqual([readResponse.body.data.collection.season]);

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation DeleteCollectionMetafield($metafields: [MetafieldIdentifierInput!]!) { metafieldsDelete(metafields: $metafields) { deletedMetafields { ownerId namespace key } userErrors { field message } } }',
        variables: {
          metafields: [{ ownerId: collection.id, namespace: 'custom', key: 'season' }],
        },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.metafieldsDelete).toEqual({
      deletedMetafields: [{ ownerId: collection.id, namespace: 'custom', key: 'season' }],
      userErrors: [],
    });

    const deletedReadResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'query CollectionMetafieldsDeleted($id: ID!) { collection(id: $id) { id season: metafield(namespace: "custom", key: "season") { id } metafields(first: 10) { nodes { id } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: {
          id: collection.id,
        },
      });

    expect(deletedReadResponse.status).toBe(200);
    expect(deletedReadResponse.body.data.collection).toEqual({
      id: collection.id,
      season: null,
      metafields: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('overlays staged collection handle lookups in live-hybrid mode when Shopify has no match yet', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { byHandle: null, byIdentifier: null } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation CreateCollection($input: CollectionInput!) { collectionCreate(input: $input) { collection { id title handle } userErrors { field message } } }',
        variables: {
          input: {
            title: 'Live Hybrid Hats',
            handle: 'live-hybrid-hats',
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.collectionCreate.userErrors).toEqual([]);
    const createdCollection = createResponse.body.data.collectionCreate.collection as {
      id: string;
      title: string;
      handle: string;
    };

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query CollectionLookup($identifier: CollectionIdentifierInput!, $handle: String!) {
          byIdentifier: collectionByIdentifier(identifier: $identifier) { id title handle }
          byHandle: collectionByHandle(handle: $handle) { id title handle }
        }`,
        variables: {
          identifier: { handle: 'live-hybrid-hats' },
          handle: 'live-hybrid-hats',
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        byIdentifier: createdCollection,
        byHandle: createdCollection,
      },
    });
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });

  it('stages collection publication visibility and publishable publish/unpublish locally', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateCollection($input: CollectionInput!) { collectionCreate(input: $input) { collection { id title publishedOnCurrentPublication publishedOnPublication(publicationId: "gid://shopify/Publication/1") availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } userErrors { field message } } }',
        variables: {
          input: {
            title: 'Publication Hats',
          },
        },
      });

    expect(createResponse.status).toBe(200);
    const collectionId = createResponse.body.data.collectionCreate.collection.id as string;
    expect(createResponse.body.data.collectionCreate).toEqual({
      collection: {
        id: collectionId,
        title: 'Publication Hats',
        publishedOnCurrentPublication: false,
        publishedOnPublication: false,
        availablePublicationsCount: {
          count: 0,
          precision: 'EXACT',
        },
        resourcePublicationsCount: {
          count: 0,
          precision: 'EXACT',
        },
      },
      userErrors: [],
    });

    const unpublishedRead = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query PublicationRead($id: ID!) { collection(id: $id) { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } published: collections(first: 10, query: "published_status:published") { nodes { id title } } unpublished: collections(first: 10, query: "published_status:unpublished") { nodes { id title } } }',
        variables: {
          id: collectionId,
        },
      });

    expect(unpublishedRead.status).toBe(200);
    expect(unpublishedRead.body.data).toEqual({
      collection: {
        id: collectionId,
        publishedOnCurrentPublication: false,
        availablePublicationsCount: {
          count: 0,
          precision: 'EXACT',
        },
        resourcePublicationsCount: {
          count: 0,
          precision: 'EXACT',
        },
      },
      published: {
        nodes: [],
      },
      unpublished: {
        nodes: [{ id: collectionId, title: 'Publication Hats' }],
      },
    });

    const publishResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation PublishCollection($id: ID!, $input: [PublicationInput!]!) { publishablePublish(id: $id, input: $input) { publishable { __typename ... on Collection { id publishedOnCurrentPublication publishedOnPublication(publicationId: "gid://shopify/Publication/1") availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } } shop { publicationCount } userErrors { field message } } }',
        variables: {
          id: collectionId,
          input: [{ publicationId: 'gid://shopify/Publication/1' }],
        },
      });

    expect(publishResponse.status).toBe(200);
    expect(publishResponse.body.data.publishablePublish).toEqual({
      publishable: {
        __typename: 'Collection',
        id: collectionId,
        publishedOnCurrentPublication: false,
        publishedOnPublication: true,
        availablePublicationsCount: {
          count: 1,
          precision: 'EXACT',
        },
        resourcePublicationsCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
      shop: {
        publicationCount: 1,
      },
      userErrors: [],
    });

    const publishedRead = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query PublishedRead($id: ID!) { collection(id: $id) { id publishedOnCurrentPublication publishedOnPublication(publicationId: "gid://shopify/Publication/1") } published: collections(first: 10, query: "published_status:published") { nodes { id title } } unpublished: collections(first: 10, query: "published_status:unpublished") { nodes { id title } } }',
        variables: {
          id: collectionId,
        },
      });

    expect(publishedRead.status).toBe(200);
    expect(publishedRead.body.data).toEqual({
      collection: {
        id: collectionId,
        publishedOnCurrentPublication: false,
        publishedOnPublication: true,
      },
      published: {
        nodes: [{ id: collectionId, title: 'Publication Hats' }],
      },
      unpublished: {
        nodes: [],
      },
    });

    const unpublishResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UnpublishCollection($id: ID!, $input: [PublicationInput!]!) { publishableUnpublish(id: $id, input: $input) { publishable { ... on Collection { id publishedOnCurrentPublication publishedOnPublication(publicationId: "gid://shopify/Publication/1") availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } } userErrors { field message } } }',
        variables: {
          id: collectionId,
          input: [{ publicationId: 'gid://shopify/Publication/1' }],
        },
      });

    expect(unpublishResponse.status).toBe(200);
    expect(unpublishResponse.body.data.publishableUnpublish).toEqual({
      publishable: {
        id: collectionId,
        publishedOnCurrentPublication: false,
        publishedOnPublication: false,
        availablePublicationsCount: {
          count: 0,
          precision: 'EXACT',
        },
        resourcePublicationsCount: {
          count: 0,
          precision: 'EXACT',
        },
      },
      userErrors: [],
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
          'query UpdatedCollection($collectionId: ID!, $productId: ID!, $handle: String!) { collection(id: $collectionId) { id title handle products(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } byIdentifier: collectionByIdentifier(identifier: { id: $collectionId }) { id title handle } byHandle: collectionByHandle(handle: $handle) { id title handle } product(id: $productId) { id collections(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: {
          collectionId: 'gid://shopify/Collection/900',
          productId: 'gid://shopify/Product/10',
          handle: 'hydrated-collection-draft',
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
        byIdentifier: {
          id: 'gid://shopify/Collection/900',
          title: 'Hydrated Collection Draft',
          handle: 'hydrated-collection-draft',
        },
        byHandle: {
          id: 'gid://shopify/Collection/900',
          title: 'Hydrated Collection Draft',
          handle: 'hydrated-collection-draft',
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
                { id: 'gid://shopify/Product/30', title: 'Green Hat', handle: 'green-hat' },
                { id: 'gid://shopify/Product/31', title: 'Blue Hat', handle: 'blue-hat' },
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
          'query AddedProducts($collectionId: ID!, $firstProductId: ID!, $secondProductId: ID!) { collection(id: $collectionId) { id title handle products(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } collections(first: 10) { nodes { id title handle products(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } } first: product(id: $firstProductId) { id collections(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } second: product(id: $secondProductId) { id collections(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } }',
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
              { id: 'gid://shopify/Product/30', title: 'Green Hat', handle: 'green-hat' },
              { id: 'gid://shopify/Product/31', title: 'Blue Hat', handle: 'blue-hat' },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
        collections: {
          nodes: [
            {
              id: 'gid://shopify/Collection/930',
              title: 'Featured Hats',
              handle: 'featured-hats',
              products: {
                nodes: [
                  { id: 'gid://shopify/Product/30', title: 'Green Hat', handle: 'green-hat' },
                  { id: 'gid://shopify/Product/31', title: 'Blue Hat', handle: 'blue-hat' },
                ],
                pageInfo: {
                  hasNextPage: false,
                  hasPreviousPage: false,
                },
              },
            },
          ],
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

  it('stages collectionAddProductsV2 locally with job payload and downstream membership visibility', async () => {
    store.upsertBaseProducts([
      makeBaseProduct('gid://shopify/Product/4320', 'V2 Green Hat', 'v2-green-hat'),
      makeBaseProduct('gid://shopify/Product/4321', 'V2 Blue Hat', 'v2-blue-hat'),
    ]);
    store.upsertBaseCollections([
      {
        id: 'gid://shopify/Collection/9432',
        title: 'V2 Featured Hats',
        handle: 'v2-featured-hats',
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp(config).callback();

    const addResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation AddProductsV2($id: ID!, $productIds: [ID!]!) { collectionAddProductsV2(id: $id, productIds: $productIds) { job { id done } userErrors { field message } } }',
        variables: {
          id: 'gid://shopify/Collection/9432',
          productIds: ['gid://shopify/Product/4320', 'gid://shopify/Product/4321'],
        },
      });

    expect(addResponse.status).toBe(200);
    expect(addResponse.body).toEqual({
      data: {
        collectionAddProductsV2: {
          job: {
            id: expect.stringMatching(/^gid:\/\/shopify\/Job\/\d+$/),
            done: false,
          },
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'query ReadCollectionMembership($id: ID!, $productId: ID!) { collection(id: $id) { id products(first: 10) { nodes { id title handle } } hasProduct(id: $productId) productsCount { count precision } } product(id: $productId) { id collections(first: 10) { nodes { id title handle } } } }',
        variables: {
          id: 'gid://shopify/Collection/9432',
          productId: 'gid://shopify/Product/4320',
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        collection: {
          id: 'gid://shopify/Collection/9432',
          products: {
            nodes: [
              { id: 'gid://shopify/Product/4321', title: 'V2 Blue Hat', handle: 'v2-blue-hat' },
              { id: 'gid://shopify/Product/4320', title: 'V2 Green Hat', handle: 'v2-green-hat' },
            ],
          },
          hasProduct: true,
          productsCount: { count: 2, precision: 'EXACT' },
        },
        product: {
          id: 'gid://shopify/Product/4320',
          collections: {
            nodes: [{ id: 'gid://shopify/Collection/9432', title: 'V2 Featured Hats', handle: 'v2-featured-hats' }],
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

  it('ignores unknown product ids when collectionAddProducts has at least one known product', async () => {
    store.upsertBaseProducts([makeBaseProduct('gid://shopify/Product/60', 'Known Product Hat', 'known-product-hat')]);
    store.upsertBaseCollections([
      {
        id: 'gid://shopify/Collection/960',
        title: 'Known Product Collection',
        handle: 'known-product-collection',
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp(config).callback();

    const addResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation AddProducts($id: ID!, $productIds: [ID!]!) { collectionAddProducts(id: $id, productIds: $productIds) { collection { id title products(first: 10) { nodes { id title } pageInfo { hasNextPage hasPreviousPage } } } userErrors { field message } } }',
        variables: {
          id: 'gid://shopify/Collection/960',
          productIds: ['gid://shopify/Product/-1', 'gid://shopify/Product/60', 'gid://shopify/Product/-2'],
        },
      });

    expect(addResponse.status).toBe(200);
    expect(addResponse.body).toEqual({
      data: {
        collectionAddProducts: {
          collection: {
            id: 'gid://shopify/Collection/960',
            title: 'Known Product Collection',
            products: {
              nodes: [{ id: 'gid://shopify/Product/60', title: 'Known Product Hat' }],
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
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns Shopify-shaped collectionAddProducts validation errors for invalid collection branches', async () => {
    store.upsertBaseProducts([makeBaseProduct('gid://shopify/Product/70', 'Validation Hat', 'validation-hat')]);
    store.upsertBaseCollections([
      {
        id: 'gid://shopify/Collection/970',
        title: 'Smart Hats',
        handle: 'smart-hats',
        isSmart: true,
      },
      {
        id: 'gid://shopify/Collection/971',
        title: 'Empty Input Hats',
        handle: 'empty-input-hats',
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp(config).callback();
    const query =
      'mutation AddProducts($id: ID!, $productIds: [ID!]!) { collectionAddProducts(id: $id, productIds: $productIds) { collection { id title } userErrors { field message } } }';

    const missingResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query,
        variables: {
          id: 'gid://shopify/Collection/-1',
          productIds: ['gid://shopify/Product/70'],
        },
      });
    const smartResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query,
        variables: {
          id: 'gid://shopify/Collection/970',
          productIds: ['gid://shopify/Product/70'],
        },
      });
    const emptyResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query,
        variables: {
          id: 'gid://shopify/Collection/971',
          productIds: [],
        },
      });

    expect(missingResponse.status).toBe(200);
    expect(missingResponse.body).toEqual({
      data: {
        collectionAddProducts: {
          collection: null,
          userErrors: [{ field: ['id'], message: 'Collection does not exist' }],
        },
      },
    });
    expect(smartResponse.status).toBe(200);
    expect(smartResponse.body).toEqual({
      data: {
        collectionAddProducts: {
          collection: null,
          userErrors: [{ field: ['id'], message: "Can't manually add products to a smart collection" }],
        },
      },
    });
    expect(emptyResponse.status).toBe(200);
    expect(emptyResponse.body).toEqual({
      data: {
        collectionAddProducts: {
          collection: null,
          userErrors: [{ field: ['productIds'], message: 'At least one product id is required' }],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages collectionReorderProducts locally with sequential manual ordering and downstream visibility', async () => {
    store.upsertBaseProducts([
      makeBaseProduct('gid://shopify/Product/80', 'Alpha Hat', 'alpha-hat'),
      makeBaseProduct('gid://shopify/Product/81', 'Bravo Hat', 'bravo-hat'),
      makeBaseProduct('gid://shopify/Product/82', 'Charlie Hat', 'charlie-hat'),
    ]);
    store.upsertBaseCollections([
      {
        id: 'gid://shopify/Collection/980',
        title: 'Manual Hats',
        handle: 'manual-hats',
        sortOrder: 'MANUAL',
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/80', [
      {
        id: 'gid://shopify/Collection/980',
        productId: 'gid://shopify/Product/80',
        title: 'Manual Hats',
        handle: 'manual-hats',
        sortOrder: 'MANUAL',
        position: 0,
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/81', [
      {
        id: 'gid://shopify/Collection/980',
        productId: 'gid://shopify/Product/81',
        title: 'Manual Hats',
        handle: 'manual-hats',
        sortOrder: 'MANUAL',
        position: 1,
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/82', [
      {
        id: 'gid://shopify/Collection/980',
        productId: 'gid://shopify/Product/82',
        title: 'Manual Hats',
        handle: 'manual-hats',
        sortOrder: 'MANUAL',
        position: 2,
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp(config).callback();

    const reorderResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation ReorderProducts($id: ID!, $moves: [MoveInput!]!) { collectionReorderProducts(id: $id, moves: $moves) { job { id done } userErrors { field message } } }',
        variables: {
          id: 'gid://shopify/Collection/980',
          moves: [
            { id: 'gid://shopify/Product/82', newPosition: '1' },
            { id: 'gid://shopify/Product/80', newPosition: '99' },
          ],
        },
      });

    expect(reorderResponse.status).toBe(200);
    expect(reorderResponse.body).toEqual({
      data: {
        collectionReorderProducts: {
          job: {
            id: expect.stringMatching(/^gid:\/\/shopify\/Job\/.+$/),
            done: false,
          },
          userErrors: [],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'query ReorderedProducts($collectionId: ID!, $productId: ID!) { collection(id: $collectionId) { id products(first: 2, sortKey: COLLECTION_DEFAULT) { edges { cursor node { id title } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } manual: products(first: 10, sortKey: MANUAL) { nodes { id title } pageInfo { hasNextPage hasPreviousPage } } } collections(first: 10) { nodes { id products(first: 10, sortKey: MANUAL) { nodes { id } pageInfo { hasNextPage hasPreviousPage } } } } product(id: $productId) { id collections(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: {
          collectionId: 'gid://shopify/Collection/980',
          productId: 'gid://shopify/Product/80',
        },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toEqual({
      data: {
        collection: {
          id: 'gid://shopify/Collection/980',
          products: {
            edges: [
              {
                cursor: 'cursor:gid://shopify/Product/82',
                node: { id: 'gid://shopify/Product/82', title: 'Charlie Hat' },
              },
              {
                cursor: 'cursor:gid://shopify/Product/81',
                node: { id: 'gid://shopify/Product/81', title: 'Bravo Hat' },
              },
            ],
            pageInfo: {
              hasNextPage: true,
              hasPreviousPage: false,
              startCursor: 'cursor:gid://shopify/Product/82',
              endCursor: 'cursor:gid://shopify/Product/81',
            },
          },
          manual: {
            nodes: [
              { id: 'gid://shopify/Product/82', title: 'Charlie Hat' },
              { id: 'gid://shopify/Product/81', title: 'Bravo Hat' },
              { id: 'gid://shopify/Product/80', title: 'Alpha Hat' },
            ],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
        collections: {
          nodes: [
            {
              id: 'gid://shopify/Collection/980',
              products: {
                nodes: [
                  { id: 'gid://shopify/Product/82' },
                  { id: 'gid://shopify/Product/81' },
                  { id: 'gid://shopify/Product/80' },
                ],
                pageInfo: {
                  hasNextPage: false,
                  hasPreviousPage: false,
                },
              },
            },
          ],
        },
        product: {
          id: 'gid://shopify/Product/80',
          collections: {
            nodes: [{ id: 'gid://shopify/Collection/980', title: 'Manual Hats', handle: 'manual-hats' }],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
            },
          },
        },
      },
    });

    const stagedMemberships = Object.values(store.getState().stagedState.productCollections).sort(
      (left, right) => (left.position ?? 0) - (right.position ?? 0),
    );
    expect(stagedMemberships.map((membership) => [membership.productId, membership.position])).toEqual([
      ['gid://shopify/Product/82', 0],
      ['gid://shopify/Product/81', 1],
      ['gid://shopify/Product/80', 2],
    ]);
    expect(store.getLog()).toMatchObject([
      {
        operationName: 'collectionReorderProducts',
        path: '/admin/api/2026-04/graphql.json',
        query:
          'mutation ReorderProducts($id: ID!, $moves: [MoveInput!]!) { collectionReorderProducts(id: $id, moves: $moves) { job { id done } userErrors { field message } } }',
        variables: {
          id: 'gid://shopify/Collection/980',
          moves: [
            { id: 'gid://shopify/Product/82', newPosition: '1' },
            { id: 'gid://shopify/Product/80', newPosition: '99' },
          ],
        },
        requestBody: {
          query:
            'mutation ReorderProducts($id: ID!, $moves: [MoveInput!]!) { collectionReorderProducts(id: $id, moves: $moves) { job { id done } userErrors { field message } } }',
          variables: {
            id: 'gid://shopify/Collection/980',
            moves: [
              { id: 'gid://shopify/Product/82', newPosition: '1' },
              { id: 'gid://shopify/Product/80', newPosition: '99' },
            ],
          },
        },
        status: 'staged',
      },
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns collectionReorderProducts user errors without staging invalid manual moves', async () => {
    store.upsertBaseProducts([
      makeBaseProduct('gid://shopify/Product/90', 'Member Hat', 'member-hat'),
      makeBaseProduct('gid://shopify/Product/91', 'Non Member Hat', 'non-member-hat'),
    ]);
    store.upsertBaseCollections([
      {
        id: 'gid://shopify/Collection/990',
        title: 'Manual Validation Hats',
        handle: 'manual-validation-hats',
        sortOrder: 'MANUAL',
      },
      {
        id: 'gid://shopify/Collection/991',
        title: 'Sorted Validation Hats',
        handle: 'sorted-validation-hats',
        sortOrder: 'ALPHA_ASC',
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/90', [
      {
        id: 'gid://shopify/Collection/990',
        productId: 'gid://shopify/Product/90',
        title: 'Manual Validation Hats',
        handle: 'manual-validation-hats',
        sortOrder: 'MANUAL',
        position: 0,
      },
    ]);

    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp(config).callback();
    const query =
      'mutation ReorderProducts($id: ID!, $moves: [MoveInput!]!) { collectionReorderProducts(id: $id, moves: $moves) { job { id done } userErrors { field message } } }';

    const missingCollectionResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query,
        variables: {
          id: 'gid://shopify/Collection/-1',
          moves: [{ id: 'gid://shopify/Product/90', newPosition: '0' }],
        },
      });
    const sortedCollectionResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query:
        'mutation { collectionReorderProducts(id: "gid://shopify/Collection/991", moves: { id: "gid://shopify/Product/90", newPosition: "0" }) { job { id done } userErrors { field message } } }',
    });
    const invalidMovesResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query,
        variables: {
          id: 'gid://shopify/Collection/990',
          moves: [
            { id: 'gid://shopify/Product/91', newPosition: '0' },
            { id: 'gid://shopify/Product/-1', newPosition: '1' },
          ],
        },
      });

    expect(missingCollectionResponse.status).toBe(200);
    expect(missingCollectionResponse.body).toEqual({
      data: {
        collectionReorderProducts: {
          job: null,
          userErrors: [{ field: ['id'], message: 'Collection not found' }],
        },
      },
    });
    expect(sortedCollectionResponse.status).toBe(200);
    expect(sortedCollectionResponse.body).toEqual({
      data: {
        collectionReorderProducts: {
          job: null,
          userErrors: [{ field: ['id'], message: "Can't reorder products unless collection is manually sorted" }],
        },
      },
    });
    expect(invalidMovesResponse.status).toBe(200);
    expect(invalidMovesResponse.body).toEqual({
      data: {
        collectionReorderProducts: {
          job: null,
          userErrors: [
            { field: ['moves', '0', 'id'], message: 'Product is not in the collection' },
            { field: ['moves', '1', 'id'], message: 'Product does not exist' },
          ],
        },
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'query ValidationOrder($collectionId: ID!) { collection(id: $collectionId) { id products(first: 10, sortKey: MANUAL) { nodes { id } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: {
          collectionId: 'gid://shopify/Collection/990',
        },
      });
    expect(readResponse.body.data.collection.products.nodes).toEqual([{ id: 'gid://shopify/Product/90' }]);
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
