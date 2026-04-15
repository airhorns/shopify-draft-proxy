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

describe('product draft flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages productCreate locally and returns it from a subsequent product query without upstream mutation', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async (input) => {
      const url = typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url;
      if (url.endsWith('/graphql.json')) {
        return new Response(
          JSON.stringify({ data: { product: null } }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        );
      }

      throw new Error(`Unexpected fetch: ${String(url)}`);
    });

    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: 'mutation productCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle status createdAt updatedAt } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Draft Hat',
            status: 'DRAFT',
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.productCreate.userErrors).toEqual([]);
    expect(createResponse.body.data.productCreate.product).toMatchObject({
      title: 'Draft Hat',
      handle: 'draft-hat',
      status: 'DRAFT',
    });

    const createdId = createResponse.body.data.productCreate.product.id as string;

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: 'query productById($id: ID!) { product(id: $id) { id title handle status createdAt updatedAt } }',
        variables: { id: createdId },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        product: {
          id: createdId,
          title: 'Draft Hat',
          handle: 'draft-hat',
          status: 'DRAFT',
          createdAt: createResponse.body.data.productCreate.product.createdAt,
          updatedAt: createResponse.body.data.productCreate.product.updatedAt,
        },
      },
    });

    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });

  it('exposes default variants and options after staged productCreate and preserves them through productUpdate', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(JSON.stringify({ data: { product: null } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    const app = createApp(config).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Variant Hat" }) { product { id title } userErrors { field message } } }',
    });

    const createdId = createResponse.body.data.productCreate.product.id as string;

    const initialQueryResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query ($id: ID!) { product(id: $id) { id title options { id name position optionValues { id name hasVariants } } variants(first: 10) { nodes { id title } pageInfo { hasNextPage hasPreviousPage } } } }',
      variables: { id: createdId },
    });

    expect(initialQueryResponse.status).toBe(200);
    expect(initialQueryResponse.body.data.product.options).toEqual([
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/ProductOption\//),
        name: 'Title',
        position: 1,
        optionValues: [
          {
            id: expect.stringMatching(/^gid:\/\/shopify\/ProductOptionValue\//),
            name: 'Default Title',
            hasVariants: true,
          },
        ],
      },
    ]);
    expect(initialQueryResponse.body.data.product.variants.nodes).toHaveLength(1);
    expect(initialQueryResponse.body.data.product.variants.nodes[0]).toMatchObject({
      id: expect.stringMatching(/^gid:\/\/shopify\/ProductVariant\//),
      title: 'Default Title',
    });
    expect(initialQueryResponse.body.data.product.variants.pageInfo).toMatchObject({
      hasNextPage: false,
      hasPreviousPage: false,
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation ($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title } userErrors { field message } } }',
      variables: {
        product: {
          id: createdId,
          title: 'Variant Hat Renamed',
        },
      },
    });

    const updatedQueryResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query ($id: ID!) { product(id: $id) { id title options { id name position optionValues { id name hasVariants } } variants(first: 10) { nodes { id title } } } }',
      variables: { id: createdId },
    });

    expect(updatedQueryResponse.status).toBe(200);
    expect(updatedQueryResponse.body.data.product.title).toBe('Variant Hat Renamed');
    expect(updatedQueryResponse.body.data.product.options).toEqual(initialQueryResponse.body.data.product.options);
    expect(updatedQueryResponse.body.data.product.variants.nodes).toEqual(initialQueryResponse.body.data.product.variants.nodes);
    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });

  it('stages tagsAdd locally for product resources and keeps downstream tag-filtered reads aligned', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateTaggedProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title tags } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Tagged Hat',
            tags: ['existing'],
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.productCreate.userErrors).toEqual([]);
    const productId = createResponse.body.data.productCreate.product.id as string;

    const addTagsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation AddTags($id: ID!, $tags: [String!]!) { tagsAdd(id: $id, tags: $tags) { node { ... on Product { id tags } } userErrors { field message } } }',
        variables: {
          id: productId,
          tags: 'existing, summer, sale',
        },
      });

    expect(addTagsResponse.status).toBe(200);
    expect(addTagsResponse.body.data.tagsAdd.userErrors).toEqual([]);
    expect(addTagsResponse.body.data.tagsAdd.node).toEqual({
      id: productId,
      tags: ['existing', 'summer', 'sale'],
    });

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query TaggedReads($id: ID!) { product(id: $id) { id tags } products(first: 10, query: "tag:sale") { nodes { id tags } } productsCount(query: "tag:sale") { count precision } }',
        variables: { id: productId },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body.data.product).toEqual({
      id: productId,
      tags: ['existing', 'summer', 'sale'],
    });
    expect(queryResponse.body.data.products.nodes).toEqual([
      {
        id: productId,
        tags: ['existing', 'summer', 'sale'],
      },
    ]);
    expect(queryResponse.body.data.productsCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });
  });

  it('stages tagsRemove locally for hydrated product resources and keeps tag-filtered reads aligned', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/900',
              title: 'Hydrated Tagged Product',
              handle: 'hydrated-tagged-product',
              status: 'ACTIVE',
              createdAt: '2024-01-01T00:00:00.000Z',
              updatedAt: '2024-01-02T00:00:00.000Z',
              vendor: 'NIKE',
              productType: 'ACCESSORIES',
              tags: ['existing', 'summer', 'sale'],
              totalInventory: 4,
              tracksInventory: true,
            },
            products: {
              nodes: [
                {
                  id: 'gid://shopify/Product/900',
                  title: 'Hydrated Tagged Product',
                  handle: 'hydrated-tagged-product',
                  status: 'ACTIVE',
                  vendor: 'NIKE',
                  productType: 'ACCESSORIES',
                  tags: ['existing', 'summer', 'sale'],
                  totalInventory: 4,
                  tracksInventory: true,
                  createdAt: '2024-01-01T00:00:00.000Z',
                  updatedAt: '2024-01-02T00:00:00.000Z',
                },
              ],
            },
            productsCount: {
              count: 1,
              precision: 'EXACT',
            },
          },
        }),
        {
          status: 200,
          headers: { 'content-type': 'application/json' },
        },
      );
    });

    const app = createApp(config).callback();

    const hydrateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query HydrateTaggedProduct($id: ID!) { product(id: $id) { id title handle status vendor productType tags totalInventory tracksInventory createdAt updatedAt } }',
        variables: { id: 'gid://shopify/Product/900' },
      });

    expect(hydrateResponse.status).toBe(200);
    expect(hydrateResponse.body.data.product.tags).toEqual(['existing', 'summer', 'sale']);

    const removeTagsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation RemoveTags($id: ID!, $tags: [String!]!) { tagsRemove(id: $id, tags: $tags) { node { ... on Product { id tags } } userErrors { field message } } }',
        variables: {
          id: 'gid://shopify/Product/900',
          tags: ['sale', 'missing'],
        },
      });

    expect(removeTagsResponse.status).toBe(200);
    expect(removeTagsResponse.body.data.tagsRemove.userErrors).toEqual([]);
    expect(removeTagsResponse.body.data.tagsRemove.node).toEqual({
      id: 'gid://shopify/Product/900',
      tags: ['existing', 'summer'],
    });

    const filteredResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query FilteredTaggedReads { remaining: products(first: 10, query: "tag:summer") { nodes { id tags } } removed: products(first: 10, query: "tag:sale") { nodes { id } } remainingCount: productsCount(query: "tag:summer") { count precision } removedCount: productsCount(query: "tag:sale") { count precision } }',
      });

    expect(filteredResponse.status).toBe(200);
    expect(filteredResponse.body.data.remaining.nodes).toEqual([
      {
        id: 'gid://shopify/Product/900',
        tags: ['existing', 'summer'],
      },
    ]);
    expect(filteredResponse.body.data.removed.nodes).toEqual([]);
    expect(filteredResponse.body.data.remainingCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });
    expect(filteredResponse.body.data.removedCount).toEqual({
      count: 0,
      precision: 'EXACT',
    });
    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });

  it('stages richer merchandising and detail fields from productCreate into downstream reads and filters', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(JSON.stringify({ data: { product: null, products: { nodes: [] } } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateRichProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title vendor productType tags descriptionHtml templateSuffix seo { title description } } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Rich Hat',
            vendor: 'NIKE',
            productType: 'ACCESSORIES',
            tags: ['cap', 'summer'],
            descriptionHtml: '<p>Rich hat description</p>',
            templateSuffix: 'custom-product',
            seo: {
              title: 'Rich Hat SEO',
              description: 'Search-ready rich hat description',
            },
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.productCreate.userErrors).toEqual([]);
    expect(createResponse.body.data.productCreate.product).toMatchObject({
      title: 'Rich Hat',
      vendor: 'NIKE',
      productType: 'ACCESSORIES',
      tags: ['cap', 'summer'],
      descriptionHtml: '<p>Rich hat description</p>',
      templateSuffix: 'custom-product',
      seo: {
        title: 'Rich Hat SEO',
        description: 'Search-ready rich hat description',
      },
    });

    const createdId = createResponse.body.data.productCreate.product.id as string;

    const productResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query RichProduct($id: ID!) { product(id: $id) { id title vendor productType tags descriptionHtml templateSuffix seo { title description } } }',
        variables: { id: createdId },
      });

    expect(productResponse.status).toBe(200);
    expect(productResponse.body.data.product).toEqual({
      id: createdId,
      title: 'Rich Hat',
      vendor: 'NIKE',
      productType: 'ACCESSORIES',
      tags: ['cap', 'summer'],
      descriptionHtml: '<p>Rich hat description</p>',
      templateSuffix: 'custom-product',
      seo: {
        title: 'Rich Hat SEO',
        description: 'Search-ready rich hat description',
      },
    });

    const filteredProductsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query { products(first: 10, query: "vendor:NIKE tag:summer product_type:ACCESSORIES") { nodes { id title vendor productType tags } } }',
      });

    expect(filteredProductsResponse.status).toBe(200);
    expect(filteredProductsResponse.body.data.products.nodes).toEqual([
      {
        id: createdId,
        title: 'Rich Hat',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: ['cap', 'summer'],
      },
    ]);
    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });

  it('applies rich merchandising/detail field updates onto hydrated products without dropping untouched detail fields', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
      const body = typeof init?.body === 'string' ? JSON.parse(init.body) as { query?: string } : {};
      const query = body.query ?? '';

      if (query.includes('product(id:')) {
        return new Response(
          JSON.stringify({
            data: {
              product: {
                id: 'gid://shopify/Product/1',
                title: 'Base Shirt',
                handle: 'base-shirt',
                status: 'ACTIVE',
                vendor: 'ADIDAS',
                productType: 'SHIRTS',
                tags: ['base', 'clearance'],
                createdAt: '2024-01-02T00:00:00.000Z',
                updatedAt: '2024-01-03T00:00:00.000Z',
                descriptionHtml: '<p>Base description</p>',
                onlineStorePreviewUrl: 'https://example.myshopify.com/products/base-shirt',
                templateSuffix: 'default',
                seo: {
                  title: 'Base SEO',
                  description: 'Base SEO description',
                },
                category: null,
              },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        );
      }

      return new Response(
        JSON.stringify({
          data: {
            products: {
              nodes: [
                {
                  id: 'gid://shopify/Product/1',
                  title: 'Base Shirt',
                  handle: 'base-shirt',
                  status: 'ACTIVE',
                  vendor: 'ADIDAS',
                  productType: 'SHIRTS',
                  tags: ['base', 'clearance'],
                  createdAt: '2024-01-02T00:00:00.000Z',
                  updatedAt: '2024-01-03T00:00:00.000Z',
                  descriptionHtml: '<p>Base description</p>',
                  onlineStorePreviewUrl: 'https://example.myshopify.com/products/base-shirt',
                  templateSuffix: 'default',
                  seo: {
                    title: 'Base SEO',
                    description: 'Base SEO description',
                  },
                  category: null,
                },
              ],
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    });

    const app = createApp(config).callback();

    const hydrateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query HydrateProduct($id: ID!) { product(id: $id) { id title vendor productType tags descriptionHtml templateSuffix seo { title description } onlineStorePreviewUrl } }',
        variables: { id: 'gid://shopify/Product/1' },
      });

    expect(hydrateResponse.status).toBe(200);
    expect(hydrateResponse.body.data.product).toMatchObject({
      id: 'gid://shopify/Product/1',
      title: 'Base Shirt',
      vendor: 'ADIDAS',
      productType: 'SHIRTS',
      tags: ['base', 'clearance'],
      descriptionHtml: '<p>Base description</p>',
      templateSuffix: 'default',
      seo: {
        title: 'Base SEO',
        description: 'Base SEO description',
      },
      onlineStorePreviewUrl: 'https://example.myshopify.com/products/base-shirt',
    });

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateRichProduct($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title vendor productType tags descriptionHtml templateSuffix seo { title description } onlineStorePreviewUrl } userErrors { field message } } }',
        variables: {
          product: {
            id: 'gid://shopify/Product/1',
            title: 'Renamed Shirt',
            vendor: 'NIKE',
            productType: 'ACCESSORIES',
            tags: ['featured', 'summer'],
            descriptionHtml: '<p>Updated description</p>',
            templateSuffix: 'summer-drop',
            seo: {
              title: 'Updated SEO',
              description: 'Updated SEO description',
            },
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.productUpdate.userErrors).toEqual([]);
    expect(updateResponse.body.data.productUpdate.product).toMatchObject({
      id: 'gid://shopify/Product/1',
      title: 'Renamed Shirt',
      vendor: 'NIKE',
      productType: 'ACCESSORIES',
      tags: ['featured', 'summer'],
      descriptionHtml: '<p>Updated description</p>',
      templateSuffix: 'summer-drop',
      seo: {
        title: 'Updated SEO',
        description: 'Updated SEO description',
      },
      onlineStorePreviewUrl: 'https://example.myshopify.com/products/base-shirt',
    });

    const productResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query UpdatedProduct($id: ID!) { product(id: $id) { id title vendor productType tags descriptionHtml templateSuffix seo { title description } onlineStorePreviewUrl } }',
        variables: { id: 'gid://shopify/Product/1' },
      });

    expect(productResponse.status).toBe(200);
    expect(productResponse.body.data.product).toEqual({
      id: 'gid://shopify/Product/1',
      title: 'Renamed Shirt',
      vendor: 'NIKE',
      productType: 'ACCESSORIES',
      tags: ['featured', 'summer'],
      descriptionHtml: '<p>Updated description</p>',
      templateSuffix: 'summer-drop',
      seo: {
        title: 'Updated SEO',
        description: 'Updated SEO description',
      },
      onlineStorePreviewUrl: 'https://example.myshopify.com/products/base-shirt',
    });

    const filteredProductsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query { products(first: 10, query: "vendor:NIKE tag:summer product_type:ACCESSORIES") { nodes { id title vendor productType tags descriptionHtml templateSuffix seo { title description } onlineStorePreviewUrl } } }',
      });

    expect(filteredProductsResponse.status).toBe(200);
    expect(filteredProductsResponse.body.data.products.nodes).toEqual([
      {
        id: 'gid://shopify/Product/1',
        title: 'Renamed Shirt',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: ['featured', 'summer'],
        descriptionHtml: '<p>Updated description</p>',
        templateSuffix: 'summer-drop',
        seo: {
          title: 'Updated SEO',
          description: 'Updated SEO description',
        },
        onlineStorePreviewUrl: 'https://example.myshopify.com/products/base-shirt',
      },
    ]);
  });

  it('overlays staged product updates and deletions onto products queries', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            products: {
              nodes: [
                {
                  id: 'gid://shopify/Product/1',
                  title: 'Base Shirt',
                  handle: 'base-shirt',
                  status: 'ACTIVE',
                  createdAt: '2024-01-02T00:00:00.000Z',
                  updatedAt: '2024-01-02T00:00:00.000Z',
                },
              ],
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );

    const app = createApp(config).callback();

    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'mutation { productUpdate(product: { id: "gid://shopify/Product/1", title: "Renamed Shirt" }) { product { id title } userErrors { field message } } }',
      });

    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'mutation { productCreate(product: { title: "New Hat" }) { product { id title } userErrors { field message } } }',
      });

    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'mutation { productDelete(input: { id: "gid://shopify/Product/1" }) { deletedProductId userErrors { field message } } }',
      });

    const productsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'query { products(first: 10) { nodes { id title handle status } } }',
      });

    expect(productsResponse.status).toBe(200);
    expect(productsResponse.body.data.products.nodes).toEqual([
      expect.objectContaining({
        title: 'New Hat',
        handle: 'new-hat',
        status: 'ACTIVE',
      }),
    ]);
  });

  it('stages productChangeStatus locally for created products and updates downstream reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(JSON.stringify({ data: { product: null, products: { nodes: [] }, productsCount: { count: 0, precision: 'EXACT' } } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'mutation { productCreate(product: { title: "Status Hat", status: DRAFT }) { product { id status } userErrors { field message } } }',
      });

    const createdId = createResponse.body.data.productCreate.product.id as string;

    const changeResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation ChangeStatus($productId: ID!, $status: ProductStatus!) { productChangeStatus(productId: $productId, status: $status) { product { id status updatedAt } userErrors { field message } } }',
        variables: {
          productId: createdId,
          status: 'ACTIVE',
        },
      });

    expect(changeResponse.status).toBe(200);
    expect(changeResponse.body.data.productChangeStatus.userErrors).toEqual([]);
    expect(changeResponse.body.data.productChangeStatus.product).toMatchObject({
      id: createdId,
      status: 'ACTIVE',
    });

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'query AfterStatusChange($id: ID!) { product(id: $id) { id status updatedAt } products(first: 10, query: "status:active") { nodes { id status } } activeCount: productsCount(query: "status:active") { count precision } }',
        variables: { id: createdId },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body.data.product).toEqual(changeResponse.body.data.productChangeStatus.product);
    expect(queryResponse.body.data.products.nodes).toEqual([
      {
        id: createdId,
        status: 'ACTIVE',
      },
    ]);
    expect(queryResponse.body.data.activeCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });

  it('overlays productChangeStatus onto hydrated products and validates missing ids', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/1',
              title: 'Base Shirt',
              handle: 'base-shirt',
              status: 'ACTIVE',
              createdAt: '2024-01-02T00:00:00.000Z',
              updatedAt: '2024-01-03T00:00:00.000Z',
            },
            products: {
              nodes: [
                {
                  id: 'gid://shopify/Product/1',
                  title: 'Base Shirt',
                  handle: 'base-shirt',
                  status: 'ACTIVE',
                  createdAt: '2024-01-02T00:00:00.000Z',
                  updatedAt: '2024-01-03T00:00:00.000Z',
                },
              ],
            },
            productsCount: {
              count: 1,
              precision: 'EXACT',
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    const invalidResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'mutation { productChangeStatus(productId: null, status: ARCHIVED) { product { id status } userErrors { field message } } }',
      });

    expect(invalidResponse.status).toBe(200);
    expect(invalidResponse.body.data.productChangeStatus).toEqual({
      product: null,
      userErrors: [{ field: ['productId'], message: 'Product id is required' }],
    });

    const hydrateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'query Hydrate($id: ID!) { product(id: $id) { id status updatedAt } }',
        variables: { id: 'gid://shopify/Product/1' },
      });

    expect(hydrateResponse.status).toBe(200);
    expect(hydrateResponse.body.data.product).toMatchObject({
      id: 'gid://shopify/Product/1',
      status: 'ACTIVE',
    });

    const changeResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation { productChangeStatus(productId: "gid://shopify/Product/1", status: ARCHIVED) { product { id status updatedAt } userErrors { field message } } }',
      });

    expect(changeResponse.status).toBe(200);
    expect(changeResponse.body.data.productChangeStatus.userErrors).toEqual([]);
    expect(changeResponse.body.data.productChangeStatus.product).toMatchObject({
      id: 'gid://shopify/Product/1',
      status: 'ARCHIVED',
    });
    expect(changeResponse.body.data.productChangeStatus.product.updatedAt).not.toBe(
      hydrateResponse.body.data.product.updatedAt,
    );

    const overlayResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query OverlayStatus($id: ID!) { product(id: $id) { id status updatedAt } archived: products(first: 10, query: "status:archived") { nodes { id status } } archivedCount: productsCount(query: "status:archived") { count precision } }',
        variables: { id: 'gid://shopify/Product/1' },
      });

    expect(overlayResponse.status).toBe(200);
    expect(overlayResponse.body.data.product).toEqual(changeResponse.body.data.productChangeStatus.product);
    expect(overlayResponse.body.data.archived.nodes).toEqual([
      {
        id: 'gid://shopify/Product/1',
        status: 'ARCHIVED',
      },
    ]);
    expect(overlayResponse.body.data.archivedCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });
  });

  it('stages productPublish and productUnpublish locally for created products with downstream publication reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(JSON.stringify({ data: { product: null } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'mutation { productCreate(product: { title: "Publication Hat", status: DRAFT }) { product { id } userErrors { field message } } }',
      });

    const productId = createResponse.body.data.productCreate.product.id as string;

    const publishResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation Publish($input: ProductPublishInput!) { productPublish(input: $input) { product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } userErrors { field message } } }',
        variables: {
          input: {
            id: productId,
            productPublications: [{ publicationId: 'gid://shopify/Publication/1' }],
          },
        },
      });

    expect(publishResponse.status).toBe(200);
    expect(publishResponse.body.data.productPublish).toEqual({
      product: {
        id: productId,
        publishedOnCurrentPublication: true,
        availablePublicationsCount: {
          count: 1,
          precision: 'EXACT',
        },
        resourcePublicationsCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
      userErrors: [],
    });

    const publishedQueryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query Published($id: ID!) { product(id: $id) { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }',
        variables: { id: productId },
      });

    expect(publishedQueryResponse.status).toBe(200);
    expect(publishedQueryResponse.body.data.product).toEqual(
      publishResponse.body.data.productPublish.product,
    );

    const unpublishResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation Unpublish($input: ProductUnpublishInput!) { productUnpublish(input: $input) { product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } userErrors { field message } } }',
        variables: {
          input: {
            id: productId,
            productPublications: [{ publicationId: 'gid://shopify/Publication/1' }],
          },
        },
      });

    expect(unpublishResponse.status).toBe(200);
    expect(unpublishResponse.body.data.productUnpublish).toEqual({
      product: {
        id: productId,
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
      userErrors: [],
    });

    const unpublishedQueryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query Unpublished($id: ID!) { product(id: $id) { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }',
        variables: { id: productId },
      });

    expect(unpublishedQueryResponse.status).toBe(200);
    expect(unpublishedQueryResponse.body.data.product).toEqual(
      unpublishResponse.body.data.productUnpublish.product,
    );
    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });

  it('overlays publication reads onto hydrated products and validates missing publish input ids', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/1',
              title: 'Base Shirt',
              handle: 'base-shirt',
              status: 'ACTIVE',
              createdAt: '2024-01-02T00:00:00.000Z',
              updatedAt: '2024-01-03T00:00:00.000Z',
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
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    const invalidPublishResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation { productPublish(input: { id: null, productPublications: [{ publicationId: "gid://shopify/Publication/1" }] }) { product { id } userErrors { field message } } }',
      });

    expect(invalidPublishResponse.status).toBe(200);
    expect(invalidPublishResponse.body.data.productPublish).toEqual({
      product: null,
      userErrors: [{ field: ['input', 'id'], message: 'Product id is required' }],
    });

    const hydrateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query HydratePublication($id: ID!) { product(id: $id) { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }',
        variables: { id: 'gid://shopify/Product/1' },
      });

    expect(hydrateResponse.status).toBe(200);
    expect(hydrateResponse.body.data.product).toMatchObject({
      id: 'gid://shopify/Product/1',
      publishedOnCurrentPublication: false,
      availablePublicationsCount: {
        count: 0,
        precision: 'EXACT',
      },
      resourcePublicationsCount: {
        count: 0,
        precision: 'EXACT',
      },
    });

    const publishResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation PublishHydrated($input: ProductPublishInput!) { productPublish(input: $input) { product { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } userErrors { field message } } }',
        variables: {
          input: {
            id: 'gid://shopify/Product/1',
            productPublications: [{ publicationId: 'gid://shopify/Publication/99' }],
          },
        },
      });

    expect(publishResponse.status).toBe(200);
    expect(publishResponse.body.data.productPublish).toEqual({
      product: {
        id: 'gid://shopify/Product/1',
        publishedOnCurrentPublication: true,
        availablePublicationsCount: {
          count: 1,
          precision: 'EXACT',
        },
        resourcePublicationsCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
      userErrors: [],
    });

    const overlayQueryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query OverlayPublication($id: ID!) { product(id: $id) { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }',
        variables: { id: 'gid://shopify/Product/1' },
      });

    expect(overlayQueryResponse.status).toBe(200);
    expect(overlayQueryResponse.body.data.product).toEqual(
      publishResponse.body.data.productPublish.product,
    );
    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });

  it('stages product option create, update, and delete mutations locally with downstream option reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(JSON.stringify({ data: { product: null } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    const app = createApp(config).callback();

    const createProductResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'mutation { productCreate(product: { title: "Option Hat" }) { product { id } userErrors { field message } } }',
      });

    const productId = createProductResponse.body.data.productCreate.product.id as string;

    const createOptionResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateOption($productId: ID!, $options: [OptionCreateInput!]!) { productOptionsCreate(productId: $productId, options: $options) { product { id options { id name position values optionValues { id name hasVariants } } } userErrors { field message } } }',
        variables: {
          productId,
          options: [
            {
              name: 'Color',
              position: 1,
              values: [{ name: 'Red' }],
            },
          ],
        },
      });

    expect(createOptionResponse.status).toBe(200);
    expect(createOptionResponse.body.data.productOptionsCreate.userErrors).toEqual([]);
    expect(createOptionResponse.body.data.productOptionsCreate.product.options).toEqual([
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/ProductOption\//),
        name: 'Color',
        position: 1,
        values: ['Red'],
        optionValues: [
          {
            id: expect.stringMatching(/^gid:\/\/shopify\/ProductOptionValue\//),
            name: 'Red',
            hasVariants: false,
          },
        ],
      },
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/ProductOption\//),
        name: 'Title',
        position: 2,
        values: ['Default Title'],
        optionValues: [
          {
            id: expect.stringMatching(/^gid:\/\/shopify\/ProductOptionValue\//),
            name: 'Default Title',
            hasVariants: true,
          },
        ],
      },
    ]);

    const colorOptionId = createOptionResponse.body.data.productOptionsCreate.product.options[0].id as string;
    const redValueId = createOptionResponse.body.data.productOptionsCreate.product.options[0].optionValues[0].id as string;

    const updateOptionResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateOption($productId: ID!, $option: OptionUpdateInput!, $optionValuesToAdd: [OptionValueCreateInput!], $optionValuesToUpdate: [OptionValueUpdateInput!]) { productOptionUpdate(productId: $productId, option: $option, optionValuesToAdd: $optionValuesToAdd, optionValuesToUpdate: $optionValuesToUpdate) { product { id options { id name position values optionValues { id name hasVariants } } } userErrors { field message } } }',
        variables: {
          productId,
          option: {
            id: colorOptionId,
            name: 'Shade',
            position: 2,
          },
          optionValuesToAdd: [{ name: 'Blue' }],
          optionValuesToUpdate: [{ id: redValueId, name: 'Crimson' }],
        },
      });

    expect(updateOptionResponse.status).toBe(200);
    expect(updateOptionResponse.body.data.productOptionUpdate.userErrors).toEqual([]);
    expect(updateOptionResponse.body.data.productOptionUpdate.product.options).toEqual([
      {
        id: expect.any(String),
        name: 'Title',
        position: 1,
        values: ['Default Title'],
        optionValues: [
          {
            id: expect.any(String),
            name: 'Default Title',
            hasVariants: true,
          },
        ],
      },
      {
        id: colorOptionId,
        name: 'Shade',
        position: 2,
        values: ['Crimson', 'Blue'],
        optionValues: [
          {
            id: redValueId,
            name: 'Crimson',
            hasVariants: false,
          },
          {
            id: expect.stringMatching(/^gid:\/\/shopify\/ProductOptionValue\//),
            name: 'Blue',
            hasVariants: false,
          },
        ],
      },
    ]);

    const deleteOptionResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeleteOption($productId: ID!, $options: [ID!]!) { productOptionsDelete(productId: $productId, options: $options) { deletedOptionsIds product { id options { id name position values optionValues { id name hasVariants } } } userErrors { field message } } }',
        variables: {
          productId,
          options: [colorOptionId],
        },
      });

    expect(deleteOptionResponse.status).toBe(200);
    expect(deleteOptionResponse.body.data.productOptionsDelete.userErrors).toEqual([]);
    expect(deleteOptionResponse.body.data.productOptionsDelete.deletedOptionsIds).toEqual([colorOptionId]);
    expect(deleteOptionResponse.body.data.productOptionsDelete.product.options).toEqual([
      {
        id: expect.any(String),
        name: 'Title',
        position: 1,
        values: ['Default Title'],
        optionValues: [
          {
            id: expect.any(String),
            name: 'Default Title',
            hasVariants: true,
          },
        ],
      },
    ]);

    const productQueryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ProductOptions($id: ID!) { product(id: $id) { id options { id name position values optionValues { id name hasVariants } } } }',
        variables: { id: productId },
      });

    expect(productQueryResponse.status).toBe(200);
    expect(productQueryResponse.body.data.product.options).toEqual([
      {
        id: expect.any(String),
        name: 'Title',
        position: 1,
        values: ['Default Title'],
        optionValues: [
          {
            id: expect.any(String),
            name: 'Default Title',
            hasVariants: true,
          },
        ],
      },
    ]);
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });

  it('stages product variant bulk create, update, and delete mutations locally with downstream variant reads and inventory-derived catalog fields', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(JSON.stringify({ data: { product: null, products: { nodes: [] }, productsCount: { count: 0, precision: 'EXACT' } } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    const app = createApp(config).callback();

    const createProductResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'mutation { productCreate(product: { title: "Variant Catalog Hat" }) { product { id } userErrors { field message } } }',
      });

    const productId = createProductResponse.body.data.productCreate.product.id as string;

    const initialVariantQuery = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity inventoryItem { id tracked requiresShipping } } } } }',
        variables: { id: productId },
      });

    const defaultVariantId = initialVariantQuery.body.data.product.variants.nodes[0].id as string;

    const fetchCountBeforeUpdate = fetchSpy.mock.calls.length;
    const updateVariantsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateVariants($productId: ID!, $variants: [ProductVariantsBulkInput!]!) { productVariantsBulkUpdate(productId: $productId, variants: $variants) { product { id totalInventory tracksInventory variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity inventoryItem { id tracked requiresShipping } } } } productVariants { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity inventoryItem { id tracked requiresShipping } } userErrors { field message } } }',
        variables: {
          productId,
          variants: [
            {
              id: defaultVariantId,
              title: 'Default / Black',
              sku: 'HAT-DEFAULT-BLACK',
              barcode: '1111111111111',
              price: '24.00',
              compareAtPrice: '30.00',
              taxable: true,
              inventoryPolicy: 'DENY',
              inventoryQuantity: 4,
              inventoryItem: {
                tracked: true,
                requiresShipping: true,
              },
            },
          ],
        },
      });

    expect(updateVariantsResponse.status).toBe(200);
    expect(fetchSpy.mock.calls.length).toBe(fetchCountBeforeUpdate);
    expect(updateVariantsResponse.body.data.productVariantsBulkUpdate.userErrors).toEqual([]);
    expect(updateVariantsResponse.body.data.productVariantsBulkUpdate.product.totalInventory).toBe(4);
    expect(updateVariantsResponse.body.data.productVariantsBulkUpdate.product.tracksInventory).toBe(true);
    expect(updateVariantsResponse.body.data.productVariantsBulkUpdate.productVariants).toEqual([
      {
        id: defaultVariantId,
        title: 'Default / Black',
        sku: 'HAT-DEFAULT-BLACK',
        barcode: '1111111111111',
        price: '24.00',
        compareAtPrice: '30.00',
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 4,
        inventoryItem: {
          id: expect.stringMatching(/^gid:\/\/shopify\/InventoryItem\//),
          tracked: true,
          requiresShipping: true,
        },
      },
    ]);

    const fetchCountBeforeCreate = fetchSpy.mock.calls.length;
    const createVariantsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateVariants($productId: ID!, $variants: [ProductVariantsBulkInput!]!) { productVariantsBulkCreate(productId: $productId, variants: $variants) { product { id totalInventory tracksInventory variants(first: 10) { nodes { id title sku barcode price inventoryQuantity inventoryItem { id tracked requiresShipping } } } } productVariants { id title sku barcode price inventoryQuantity inventoryItem { id tracked requiresShipping } } userErrors { field message } } }',
        variables: {
          productId,
          variants: [
            {
              title: 'Blue',
              sku: 'HAT-BLUE',
              barcode: '2222222222222',
              price: '26.00',
              inventoryQuantity: 6,
              inventoryItem: {
                tracked: true,
                requiresShipping: false,
              },
            },
          ],
        },
      });

    expect(createVariantsResponse.status).toBe(200);
    expect(fetchSpy.mock.calls.length).toBe(fetchCountBeforeCreate);
    expect(createVariantsResponse.body.data.productVariantsBulkCreate.userErrors).toEqual([]);
    expect(createVariantsResponse.body.data.productVariantsBulkCreate.product.totalInventory).toBe(10);
    expect(createVariantsResponse.body.data.productVariantsBulkCreate.product.tracksInventory).toBe(true);
    expect(createVariantsResponse.body.data.productVariantsBulkCreate.productVariants).toEqual([
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/ProductVariant\//),
        title: 'Blue',
        sku: 'HAT-BLUE',
        barcode: '2222222222222',
        price: '26.00',
        inventoryQuantity: 6,
        inventoryItem: {
          id: expect.stringMatching(/^gid:\/\/shopify\/InventoryItem\//),
          tracked: true,
          requiresShipping: false,
        },
      },
    ]);

    const createdVariantId = createVariantsResponse.body.data.productVariantsBulkCreate.productVariants[0].id as string;

    const catalogResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id totalInventory tracksInventory variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity inventoryItem { id tracked requiresShipping } } } } products(first: 10, query: "sku:HAT-BLUE") { nodes { id totalInventory tracksInventory } } skuCount: productsCount(query: "sku:HAT-BLUE") { count precision } }',
        variables: { id: productId },
      });

    expect(catalogResponse.status).toBe(200);
    expect(catalogResponse.body.data.product).toMatchObject({
      id: productId,
      totalInventory: 10,
      tracksInventory: true,
    });
    expect(catalogResponse.body.data.product.variants.nodes).toEqual([
      {
        id: defaultVariantId,
        title: 'Default / Black',
        sku: 'HAT-DEFAULT-BLACK',
        barcode: '1111111111111',
        price: '24.00',
        compareAtPrice: '30.00',
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 4,
        inventoryItem: {
          id: expect.stringMatching(/^gid:\/\/shopify\/InventoryItem\//),
          tracked: true,
          requiresShipping: true,
        },
      },
      {
        id: createdVariantId,
        title: 'Blue',
        sku: 'HAT-BLUE',
        barcode: '2222222222222',
        price: '26.00',
        compareAtPrice: null,
        taxable: null,
        inventoryPolicy: null,
        inventoryQuantity: 6,
        inventoryItem: {
          id: expect.stringMatching(/^gid:\/\/shopify\/InventoryItem\//),
          tracked: true,
          requiresShipping: false,
        },
      },
    ]);
    expect(catalogResponse.body.data.products.nodes).toEqual([
      {
        id: productId,
        totalInventory: 10,
        tracksInventory: true,
      },
    ]);
    expect(catalogResponse.body.data.skuCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });

    const fetchCountBeforeDelete = fetchSpy.mock.calls.length;
    const deleteVariantsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeleteVariants($productId: ID!, $variantsIds: [ID!]!) { productVariantsBulkDelete(productId: $productId, variantsIds: $variantsIds) { product { id totalInventory tracksInventory variants(first: 10) { nodes { id title sku inventoryQuantity } } } userErrors { field message } } }',
        variables: {
          productId,
          variantsIds: [defaultVariantId],
        },
      });

    expect(deleteVariantsResponse.status).toBe(200);
    expect(fetchSpy.mock.calls.length).toBe(fetchCountBeforeDelete);
    expect(deleteVariantsResponse.body.data.productVariantsBulkDelete.userErrors).toEqual([]);
    expect(deleteVariantsResponse.body.data.productVariantsBulkDelete.product).toEqual({
      id: productId,
      totalInventory: 6,
      tracksInventory: true,
      variants: {
        nodes: [
          {
            id: createdVariantId,
            title: 'Blue',
            sku: 'HAT-BLUE',
            inventoryQuantity: 6,
          },
        ],
      },
    });

    const afterDeleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id totalInventory tracksInventory variants(first: 10) { nodes { id title sku inventoryQuantity } } } deletedSkuCount: productsCount(query: "sku:HAT-DEFAULT-BLACK") { count precision } activeSkuCount: productsCount(query: "sku:HAT-BLUE") { count precision } }',
        variables: { id: productId },
      });

    expect(afterDeleteResponse.status).toBe(200);
    expect(afterDeleteResponse.body.data.product).toEqual({
      id: productId,
      totalInventory: 6,
      tracksInventory: true,
      variants: {
        nodes: [
          {
            id: createdVariantId,
            title: 'Blue',
            sku: 'HAT-BLUE',
            inventoryQuantity: 6,
          },
        ],
      },
    });
    expect(afterDeleteResponse.body.data.deletedSkuCount).toEqual({
      count: 0,
      precision: 'EXACT',
    });
    expect(afterDeleteResponse.body.data.activeSkuCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });
  });

  it('stages singular product variant create, update, and delete mutations locally via the same overlay model', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(JSON.stringify({ data: { product: null, products: { nodes: [] }, productsCount: { count: 0, precision: 'EXACT' } } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    const app = createApp(config).callback();

    const createProductResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'mutation { productCreate(product: { title: "Single Variant Hat" }) { product { id } userErrors { field message } } }',
      });

    const productId = createProductResponse.body.data.productCreate.product.id as string;

    const initialVariantQuery = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id variants(first: 10) { nodes { id title sku barcode price inventoryQuantity inventoryItem { id tracked requiresShipping } } } } }',
        variables: { id: productId },
      });

    const defaultVariantId = initialVariantQuery.body.data.product.variants.nodes[0].id as string;

    const fetchCountBeforeCreate = fetchSpy.mock.calls.length;
    const createVariantResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateVariant($input: ProductVariantInput!) { productVariantCreate(input: $input) { product { id totalInventory tracksInventory } productVariant { id title sku barcode price inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } userErrors { field message } } }',
        variables: {
          input: {
            productId,
            title: 'Blue / Large',
            sku: 'SVH-BL-L',
            barcode: '3333333333333',
            price: '29.00',
            inventoryQuantity: 8,
            selectedOptions: [
              { name: 'Color', value: 'Blue' },
              { name: 'Size', value: 'Large' },
            ],
            inventoryItem: {
              tracked: true,
              requiresShipping: false,
            },
          },
        },
      });

    expect(createVariantResponse.status).toBe(200);
    expect(fetchSpy.mock.calls.length).toBe(fetchCountBeforeCreate);
    expect(createVariantResponse.body.data.productVariantCreate.userErrors).toEqual([]);
    expect(createVariantResponse.body.data.productVariantCreate.product).toEqual({
      id: productId,
      totalInventory: 8,
      tracksInventory: true,
    });
    expect(createVariantResponse.body.data.productVariantCreate.productVariant).toEqual({
      id: expect.stringMatching(/^gid:\/\/shopify\/ProductVariant\//),
      title: 'Blue / Large',
      sku: 'SVH-BL-L',
      barcode: '3333333333333',
      price: '29.00',
      inventoryQuantity: 8,
      selectedOptions: [
        { name: 'Color', value: 'Blue' },
        { name: 'Size', value: 'Large' },
      ],
      inventoryItem: {
        id: expect.stringMatching(/^gid:\/\/shopify\/InventoryItem\//),
        tracked: true,
        requiresShipping: false,
      },
    });

    const createdVariantId = createVariantResponse.body.data.productVariantCreate.productVariant.id as string;

    const fetchCountBeforeUpdate = fetchSpy.mock.calls.length;
    const updateVariantResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateVariant($input: ProductVariantInput!) { productVariantUpdate(input: $input) { product { id totalInventory tracksInventory } productVariant { id title sku barcode price inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } userErrors { field message } } }',
        variables: {
          input: {
            id: createdVariantId,
            title: 'Blue / XL',
            sku: 'SVH-BL-XL',
            inventoryQuantity: 5,
            selectedOptions: [
              { name: 'Color', value: 'Blue' },
              { name: 'Size', value: 'XL' },
            ],
            inventoryItem: {
              tracked: true,
              requiresShipping: true,
            },
          },
        },
      });

    expect(updateVariantResponse.status).toBe(200);
    expect(fetchSpy.mock.calls.length).toBe(fetchCountBeforeUpdate);
    expect(updateVariantResponse.body.data.productVariantUpdate.userErrors).toEqual([]);
    expect(updateVariantResponse.body.data.productVariantUpdate.product).toEqual({
      id: productId,
      totalInventory: 5,
      tracksInventory: true,
    });
    expect(updateVariantResponse.body.data.productVariantUpdate.productVariant).toEqual({
      id: createdVariantId,
      title: 'Blue / XL',
      sku: 'SVH-BL-XL',
      barcode: '3333333333333',
      price: '29.00',
      inventoryQuantity: 5,
      selectedOptions: [
        { name: 'Color', value: 'Blue' },
        { name: 'Size', value: 'XL' },
      ],
      inventoryItem: {
        id: expect.stringMatching(/^gid:\/\/shopify\/InventoryItem\//),
        tracked: true,
        requiresShipping: true,
      },
    });

    const downstreamQuery = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id totalInventory tracksInventory variants(first: 10) { nodes { id title sku inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } } products(first: 10, query: "sku:SVH-BL-XL") { nodes { id totalInventory tracksInventory } } skuCount: productsCount(query: "sku:SVH-BL-XL") { count precision } }',
        variables: { id: productId },
      });

    expect(downstreamQuery.status).toBe(200);
    expect(downstreamQuery.body.data.product).toEqual({
      id: productId,
      totalInventory: 5,
      tracksInventory: true,
      variants: {
        nodes: [
          {
            id: defaultVariantId,
            title: 'Default Title',
            sku: null,
            inventoryQuantity: null,
            selectedOptions: [],
            inventoryItem: null,
          },
          {
            id: createdVariantId,
            title: 'Blue / XL',
            sku: 'SVH-BL-XL',
            inventoryQuantity: 5,
            selectedOptions: [
              { name: 'Color', value: 'Blue' },
              { name: 'Size', value: 'XL' },
            ],
            inventoryItem: {
              id: expect.stringMatching(/^gid:\/\/shopify\/InventoryItem\//),
              tracked: true,
              requiresShipping: true,
            },
          },
        ],
      },
    });
    expect(downstreamQuery.body.data.products.nodes).toEqual([
      {
        id: productId,
        totalInventory: 5,
        tracksInventory: true,
      },
    ]);
    expect(downstreamQuery.body.data.skuCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });

    const fetchCountBeforeDelete = fetchSpy.mock.calls.length;
    const deleteVariantResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeleteVariant($id: ID!) { productVariantDelete(id: $id) { deletedProductVariantId userErrors { field message } } }',
        variables: { id: createdVariantId },
      });

    expect(deleteVariantResponse.status).toBe(200);
    expect(fetchSpy.mock.calls.length).toBe(fetchCountBeforeDelete);
    expect(deleteVariantResponse.body.data.productVariantDelete).toEqual({
      deletedProductVariantId: createdVariantId,
      userErrors: [],
    });

    const afterDeleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id totalInventory tracksInventory variants(first: 10) { nodes { id title sku inventoryQuantity } } } deletedSkuCount: productsCount(query: "sku:SVH-BL-XL") { count precision } }',
        variables: { id: productId },
      });

    expect(afterDeleteResponse.status).toBe(200);
    expect(afterDeleteResponse.body.data.product).toEqual({
      id: productId,
      totalInventory: null,
      tracksInventory: null,
      variants: {
        nodes: [
          {
            id: defaultVariantId,
            title: 'Default Title',
            sku: null,
            inventoryQuantity: null,
          },
        ],
      },
    });
    expect(afterDeleteResponse.body.data.deletedSkuCount).toEqual({
      count: 0,
      precision: 'EXACT',
    });
  });

  it('keeps option values and hasVariants in sync with singular variant selectedOptions mutations', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(JSON.stringify({ data: { product: null, products: { nodes: [] }, productsCount: { count: 0, precision: 'EXACT' } } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    const app = createApp(config).callback();

    const createProductResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'mutation { productCreate(product: { title: "Option Synced Hat" }) { product { id } userErrors { field message } } }',
      });

    const productId = createProductResponse.body.data.productCreate.product.id as string;

    const createOptionsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateOptions($productId: ID!, $options: [OptionCreateInput!]!) { productOptionsCreate(productId: $productId, options: $options) { product { id options { id name position values optionValues { id name hasVariants } } } userErrors { field message } } }',
        variables: {
          productId,
          options: [
            { name: 'Color', values: [{ name: 'Red' }, { name: 'Blue' }] },
            { name: 'Size', values: [{ name: 'Small' }, { name: 'Large' }] },
          ],
        },
      });

    expect(createOptionsResponse.status).toBe(200);
    expect(createOptionsResponse.body.data.productOptionsCreate.userErrors).toEqual([]);

    const createVariantResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateVariant($input: ProductVariantInput!) { productVariantCreate(input: $input) { productVariant { id } userErrors { field message } } }',
        variables: {
          input: {
            productId,
            selectedOptions: [
              { name: 'Color', value: 'Blue' },
              { name: 'Size', value: 'Large' },
            ],
          },
        },
      });

    expect(createVariantResponse.status).toBe(200);
    expect(createVariantResponse.body.data.productVariantCreate.userErrors).toEqual([]);

    const createdVariantId = createVariantResponse.body.data.productVariantCreate.productVariant.id as string;

    const optionsAfterCreate = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ProductOptions($id: ID!) { product(id: $id) { id options { name values optionValues { name hasVariants } } } }',
        variables: { id: productId },
      });

    expect(optionsAfterCreate.status).toBe(200);
    expect(optionsAfterCreate.body.data.product.options).toEqual(
      expect.arrayContaining([
        {
          name: 'Color',
          values: ['Red', 'Blue'],
          optionValues: [
            { name: 'Red', hasVariants: false },
            { name: 'Blue', hasVariants: true },
          ],
        },
        {
          name: 'Size',
          values: ['Small', 'Large'],
          optionValues: [
            { name: 'Small', hasVariants: false },
            { name: 'Large', hasVariants: true },
          ],
        },
      ]),
    );

    const updateVariantResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateVariant($input: ProductVariantInput!) { productVariantUpdate(input: $input) { productVariant { id } userErrors { field message } } }',
        variables: {
          input: {
            id: createdVariantId,
            selectedOptions: [
              { name: 'Color', value: 'Red' },
              { name: 'Size', value: 'Large' },
            ],
          },
        },
      });

    expect(updateVariantResponse.status).toBe(200);
    expect(updateVariantResponse.body.data.productVariantUpdate.userErrors).toEqual([]);

    const optionsAfterUpdate = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ProductOptions($id: ID!) { product(id: $id) { id options { name optionValues { name hasVariants } } } }',
        variables: { id: productId },
      });

    expect(optionsAfterUpdate.status).toBe(200);
    expect(optionsAfterUpdate.body.data.product.options).toEqual(
      expect.arrayContaining([
        {
          name: 'Color',
          optionValues: [
            { name: 'Red', hasVariants: true },
            { name: 'Blue', hasVariants: false },
          ],
        },
        {
          name: 'Size',
          optionValues: [
            { name: 'Small', hasVariants: false },
            { name: 'Large', hasVariants: true },
          ],
        },
      ]),
    );

    const deleteVariantResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'mutation DeleteVariant($id: ID!) { productVariantDelete(id: $id) { deletedProductVariantId userErrors { field message } } }',
        variables: { id: createdVariantId },
      });

    expect(deleteVariantResponse.status).toBe(200);
    expect(deleteVariantResponse.body.data.productVariantDelete.userErrors).toEqual([]);

    const optionsAfterDelete = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ProductOptions($id: ID!) { product(id: $id) { id options { name optionValues { name hasVariants } } } }',
        variables: { id: productId },
      });

    expect(optionsAfterDelete.status).toBe(200);
    expect(optionsAfterDelete.body.data.product.options).toEqual(
      expect.arrayContaining([
        {
          name: 'Color',
          optionValues: [
            { name: 'Red', hasVariants: false },
            { name: 'Blue', hasVariants: false },
          ],
        },
        {
          name: 'Size',
          optionValues: [
            { name: 'Small', hasVariants: false },
            { name: 'Large', hasVariants: false },
          ],
        },
      ]),
    );
  });

  it('adds missing option values and updates hasVariants during bulk variant mutations', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(JSON.stringify({ data: { product: null } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createProductResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'mutation { productCreate(product: { title: "Bulk Option Hat" }) { product { id } userErrors { field message } } }',
      });

    const productId = createProductResponse.body.data.productCreate.product.id as string;

    const createOptionsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateOptions($productId: ID!, $options: [OptionCreateInput!]!) { productOptionsCreate(productId: $productId, options: $options) { product { id options { name optionValues { name hasVariants } } } userErrors { field message } } }',
        variables: {
          productId,
          options: [{ name: 'Color', values: [{ name: 'Red' }] }],
        },
      });

    expect(createOptionsResponse.status).toBe(200);
    expect(createOptionsResponse.body.data.productOptionsCreate.userErrors).toEqual([]);

    const createVariantsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation BulkCreate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) { productVariantsBulkCreate(productId: $productId, variants: $variants) { productVariants { id } userErrors { field message } } }',
        variables: {
          productId,
          variants: [
            { selectedOptions: [{ name: 'Color', value: 'Blue' }] },
            { selectedOptions: [{ name: 'Color', value: 'Red' }] },
          ],
        },
      });

    expect(createVariantsResponse.status).toBe(200);
    expect(createVariantsResponse.body.data.productVariantsBulkCreate.userErrors).toEqual([]);

    const createdVariantIds = createVariantsResponse.body.data.productVariantsBulkCreate.productVariants.map(
      (variant: { id: string }) => variant.id,
    ) as string[];

    const optionsAfterCreate = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ProductOptions($id: ID!) { product(id: $id) { id options { name values optionValues { name hasVariants } } } }',
        variables: { id: productId },
      });

    expect(optionsAfterCreate.status).toBe(200);
    expect(optionsAfterCreate.body.data.product.options).toEqual(
      expect.arrayContaining([
        {
          name: 'Color',
          values: ['Red', 'Blue'],
          optionValues: [
            { name: 'Red', hasVariants: true },
            { name: 'Blue', hasVariants: true },
          ],
        },
      ]),
    );

    const deleteBlueVariantResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation BulkDelete($productId: ID!, $variantsIds: [ID!]!) { productVariantsBulkDelete(productId: $productId, variantsIds: $variantsIds) { userErrors { field message } } }',
        variables: { productId, variantsIds: [createdVariantIds[0]] },
      });

    expect(deleteBlueVariantResponse.status).toBe(200);
    expect(deleteBlueVariantResponse.body.data.productVariantsBulkDelete.userErrors).toEqual([]);

    const optionsAfterDelete = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ProductOptions($id: ID!) { product(id: $id) { id options { name optionValues { name hasVariants } } } }',
        variables: { id: productId },
      });

    expect(optionsAfterDelete.status).toBe(200);
    expect(optionsAfterDelete.body.data.product.options).toEqual(
      expect.arrayContaining([
        {
          name: 'Color',
          optionValues: [
            { name: 'Red', hasVariants: true },
            { name: 'Blue', hasVariants: false },
          ],
        },
      ]),
    );
  });

  it('stages metafieldsSet locally for product metafields and exposes the new values on downstream reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(JSON.stringify({ data: { product: null } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'mutation { productCreate(product: { title: "Metafield Hat" }) { product { id } userErrors { field message } } }',
      });

    const productId = createResponse.body.data.productCreate.product.id as string;
    const fetchCountBeforeMutation = fetchSpy.mock.calls.length;

    const setResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation SetMetafields($metafields: [MetafieldsSetInput!]!) { metafieldsSet(metafields: $metafields) { metafields { id namespace key type value } userErrors { field message } } }',
        variables: {
          metafields: [
            {
              ownerId: productId,
              namespace: 'custom',
              key: 'material',
              type: 'single_line_text_field',
              value: 'Canvas',
            },
            {
              ownerId: productId,
              namespace: 'details',
              key: 'origin',
              type: 'single_line_text_field',
              value: 'VN',
            },
          ],
        },
      });

    expect(setResponse.status).toBe(200);
    expect(fetchSpy.mock.calls.length).toBe(fetchCountBeforeMutation);
    expect(setResponse.body.data.metafieldsSet.userErrors).toEqual([]);
    expect(setResponse.body.data.metafieldsSet.metafields).toEqual([
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/Metafield\//),
        namespace: 'custom',
        key: 'material',
        type: 'single_line_text_field',
        value: 'Canvas',
      },
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/Metafield\//),
        namespace: 'details',
        key: 'origin',
        type: 'single_line_text_field',
        value: 'VN',
      },
    ]);

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id primarySpec: metafield(namespace: "custom", key: "material") { id namespace key type value } metafields(first: 10) { nodes { id namespace key type value } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: productId },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body.data.product.primarySpec).toEqual(setResponse.body.data.metafieldsSet.metafields[0]);
    expect(queryResponse.body.data.product.metafields).toEqual({
      nodes: setResponse.body.data.metafieldsSet.metafields,
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: `cursor:${setResponse.body.data.metafieldsSet.metafields[0].id}`,
        endCursor: `cursor:${setResponse.body.data.metafieldsSet.metafields[1].id}`,
      },
    });
  });

  it('stages metafieldDelete locally against hydrated product metafields and removes the deleted metafield from downstream reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
      const body = typeof init?.body === 'string' ? JSON.parse(init.body) as { query?: string } : {};
      const query = body.query ?? '';

      if (query.includes('product(id:')) {
        return new Response(
          JSON.stringify({
            data: {
              product: {
                id: 'gid://shopify/Product/1',
                title: 'Hydrated Metafield Hat',
                handle: 'hydrated-metafield-hat',
                status: 'ACTIVE',
                createdAt: '2024-01-01T00:00:00.000Z',
                updatedAt: '2024-01-02T00:00:00.000Z',
                metafields: {
                  edges: [
                    {
                      node: {
                        id: 'gid://shopify/Metafield/9001',
                        namespace: 'custom',
                        key: 'material',
                        type: 'single_line_text_field',
                        value: 'Canvas',
                      },
                    },
                    {
                      node: {
                        id: 'gid://shopify/Metafield/9002',
                        namespace: 'details',
                        key: 'origin',
                        type: 'single_line_text_field',
                        value: 'VN',
                      },
                    },
                  ],
                },
              },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        );
      }

      throw new Error(`Unexpected fetch during metafieldDelete test: ${String(input)}`);
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    const hydrateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id metafields(first: 10) { edges { node { id namespace key type value } } } } }',
        variables: { id: 'gid://shopify/Product/1' },
      });

    expect(hydrateResponse.status).toBe(200);
    expect(hydrateResponse.body.data.product.metafields.edges).toEqual([
      {
        node: {
          id: 'gid://shopify/Metafield/9001',
          namespace: 'custom',
          key: 'material',
          type: 'single_line_text_field',
          value: 'Canvas',
        },
      },
      {
        node: {
          id: 'gid://shopify/Metafield/9002',
          namespace: 'details',
          key: 'origin',
          type: 'single_line_text_field',
          value: 'VN',
        },
      },
    ]);

    const deleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeleteMetafield($input: MetafieldDeleteInput!) { metafieldDelete(input: $input) { deletedId userErrors { field message } } }',
        variables: {
          input: { id: 'gid://shopify/Metafield/9001' },
        },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.metafieldDelete).toEqual({
      deletedId: 'gid://shopify/Metafield/9001',
      userErrors: [],
    });

    const overlayResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id metafield(namespace: "custom", key: "material") { id namespace key type value } metafields(first: 10) { nodes { id namespace key type value } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: 'gid://shopify/Product/1' },
      });

    expect(overlayResponse.status).toBe(200);
    expect(overlayResponse.body.data.product.metafield).toBeNull();
    expect(overlayResponse.body.data.product.metafields).toEqual({
      nodes: [
        {
          id: 'gid://shopify/Metafield/9002',
          namespace: 'details',
          key: 'origin',
          type: 'single_line_text_field',
          value: 'VN',
        },
      ],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/Metafield/9002',
        endCursor: 'cursor:gid://shopify/Metafield/9002',
      },
    });
  });

  it('returns Shopify-like empty defaults in snapshot mode when no product exists', async () => {
    const snapshotApp = createApp({ ...config, readMode: 'snapshot' }).callback();

    const productResponse = await request(snapshotApp)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'query productById($id: ID!) { product(id: $id) { id title } }',
        variables: { id: 'gid://shopify/Product/404' },
      });

    const productsResponse = await request(snapshotApp)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'query { products(first: 10) { nodes { id title } } }',
      });

    expect(productResponse.body).toEqual({ data: { product: null } });
    expect(productsResponse.body).toEqual({
      data: {
        products: {
          nodes: [],
        },
      },
    });
  });
  it('duplicates the effective local product graph into a new staged product without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/100',
        legacyResourceId: '100',
        title: 'Base Shoe',
        handle: 'base-shoe',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-02T00:00:00.000Z',
        updatedAt: '2024-01-03T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'SHOES',
        tags: ['running', 'summer'],
        totalInventory: 8,
        tracksInventory: true,
        descriptionHtml: '<p>Base shoe description</p>',
        onlineStorePreviewUrl: 'https://example.myshopify.com/products/base-shoe',
        templateSuffix: 'performance',
        seo: {
          title: 'Base Shoe SEO',
          description: 'Base Shoe SEO description',
        },
        category: null,
      },
    ]);
    store.replaceBaseOptionsForProduct('gid://shopify/Product/100', [
      {
        id: 'gid://shopify/ProductOption/1000',
        productId: 'gid://shopify/Product/100',
        name: 'Size',
        position: 1,
        optionValues: [
          {
            id: 'gid://shopify/ProductOptionValue/1001',
            name: '8',
            hasVariants: true,
          },
        ],
      },
    ]);
    store.replaceBaseVariantsForProduct('gid://shopify/Product/100', [
      {
        id: 'gid://shopify/ProductVariant/2000',
        productId: 'gid://shopify/Product/100',
        title: '8',
        sku: 'BASE-8',
        barcode: '12345678',
        price: '70.00',
        compareAtPrice: '80.00',
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 8,
        selectedOptions: [{ name: 'Size', value: '8' }],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/3000',
          tracked: true,
          requiresShipping: true,
          measurement: {
            weight: {
              unit: 'KILOGRAMS',
              value: 1.4,
            },
          },
          countryCodeOfOrigin: 'US',
          provinceCodeOfOrigin: 'CA',
          harmonizedSystemCode: '640411',
        },
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/100', [
      {
        id: 'gid://shopify/Collection/4000',
        productId: 'gid://shopify/Product/100',
        title: 'Summer',
        handle: 'summer',
      },
    ]);
    store.replaceBaseMediaForProduct('gid://shopify/Product/100', [
      {
        key: 'gid://shopify/Product/100:media:0',
        productId: 'gid://shopify/Product/100',
        position: 0,
        mediaContentType: 'IMAGE',
        alt: 'Base image',
        previewImageUrl: 'https://cdn.example.com/base-shoe.jpg',
      },
    ]);
    store.replaceBaseMetafieldsForProduct('gid://shopify/Product/100', [
      {
        id: 'gid://shopify/Metafield/5000',
        productId: 'gid://shopify/Product/100',
        namespace: 'custom',
        key: 'material',
        type: 'single_line_text_field',
        value: 'mesh',
      },
    ]);

    const duplicateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DuplicateProduct($productId: ID!, $newTitle: String!) { productDuplicate(productId: $productId, newTitle: $newTitle) { newProduct { id title handle status vendor productType tags descriptionHtml templateSuffix seo { title description } onlineStorePreviewUrl options { id name position values optionValues { id name hasVariants } } variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping measurement { weight { unit value } } countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode } } } collections(first: 10) { nodes { id title handle } } media(first: 10) { nodes { mediaContentType alt preview { image { url } } } } metafield(namespace: "custom", key: "material") { id namespace key type value } metafields(first: 10) { nodes { id namespace key type value } } } userErrors { field message } } }',
        variables: {
          productId: 'gid://shopify/Product/100',
          newTitle: 'Copied Shoe',
        },
      });

    expect(duplicateResponse.status).toBe(200);
    expect(duplicateResponse.body.data.productDuplicate.userErrors).toEqual([]);
    expect(duplicateResponse.body.data.productDuplicate.newProduct).toMatchObject({
      title: 'Copied Shoe',
      handle: 'copied-shoe',
      status: 'DRAFT',
      vendor: 'NIKE',
      productType: 'SHOES',
      tags: ['running', 'summer'],
      descriptionHtml: '<p>Base shoe description</p>',
      templateSuffix: 'performance',
      seo: {
        title: 'Base Shoe SEO',
        description: 'Base Shoe SEO description',
      },
      onlineStorePreviewUrl: 'https://example.myshopify.com/products/base-shoe',
    });

    const duplicatedProductId = duplicateResponse.body.data.productDuplicate.newProduct.id as string;
    expect(duplicatedProductId).not.toBe('gid://shopify/Product/100');
    expect(duplicateResponse.body.data.productDuplicate.newProduct.options).toEqual([
      {
        id: expect.not.stringContaining('gid://shopify/ProductOption/1000'),
        name: 'Size',
        position: 1,
        values: ['8'],
        optionValues: [
          {
            id: expect.not.stringContaining('gid://shopify/ProductOptionValue/1001'),
            name: '8',
            hasVariants: true,
          },
        ],
      },
    ]);
    expect(duplicateResponse.body.data.productDuplicate.newProduct.variants.nodes).toEqual([
      {
        id: expect.not.stringContaining('gid://shopify/ProductVariant/2000'),
        title: '8',
        sku: 'BASE-8',
        barcode: '12345678',
        price: '70.00',
        compareAtPrice: '80.00',
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 8,
        selectedOptions: [{ name: 'Size', value: '8' }],
        inventoryItem: {
          id: expect.not.stringContaining('gid://shopify/InventoryItem/3000'),
          tracked: true,
          requiresShipping: true,
          measurement: {
            weight: {
              unit: 'KILOGRAMS',
              value: 1.4,
            },
          },
          countryCodeOfOrigin: 'US',
          provinceCodeOfOrigin: 'CA',
          harmonizedSystemCode: '640411',
        },
      },
    ]);
    expect(duplicateResponse.body.data.productDuplicate.newProduct.collections.nodes).toEqual([
      {
        id: 'gid://shopify/Collection/4000',
        title: 'Summer',
        handle: 'summer',
      },
    ]);
    expect(duplicateResponse.body.data.productDuplicate.newProduct.media.nodes).toEqual([
      {
        mediaContentType: 'IMAGE',
        alt: 'Base image',
        preview: {
          image: {
            url: 'https://cdn.example.com/base-shoe.jpg',
          },
        },
      },
    ]);
    expect(duplicateResponse.body.data.productDuplicate.newProduct.metafield).toMatchObject({
      namespace: 'custom',
      key: 'material',
      type: 'single_line_text_field',
      value: 'mesh',
    });
    expect(duplicateResponse.body.data.productDuplicate.newProduct.metafields.nodes).toEqual([
      {
        id: expect.not.stringContaining('gid://shopify/Metafield/5000'),
        namespace: 'custom',
        key: 'material',
        type: 'single_line_text_field',
        value: 'mesh',
      },
    ]);

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { duplicated: product(id: $id) { id title handle status options { name values } variants(first: 10) { nodes { title sku inventoryItem { tracked } } } collections(first: 10) { nodes { id title handle } } media(first: 10) { nodes { mediaContentType alt preview { image { url } } } } metafield(namespace: "custom", key: "material") { namespace key value } } total: productsCount { count precision } }',
        variables: { id: duplicatedProductId },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        duplicated: {
          id: duplicatedProductId,
          title: 'Copied Shoe',
          handle: 'copied-shoe',
          status: 'DRAFT',
          options: [{ name: 'Size', values: ['8'] }],
          variants: {
            nodes: [
              {
                title: '8',
                sku: 'BASE-8',
                inventoryItem: {
                  tracked: true,
                },
              },
            ],
          },
          collections: {
            nodes: [{ id: 'gid://shopify/Collection/4000', title: 'Summer', handle: 'summer' }],
          },
          media: {
            nodes: [
              {
                mediaContentType: 'IMAGE',
                alt: 'Base image',
                preview: {
                  image: {
                    url: 'https://cdn.example.com/base-shoe.jpg',
                  },
                },
              },
            ],
          },
          metafield: {
            namespace: 'custom',
            key: 'material',
            value: 'mesh',
          },
        },
        total: {
          count: 2,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages product media create, update, and delete locally with downstream media reads and inline fragment image fields', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createProductResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'mutation { productCreate(product: { title: "Media Hat" }) { product { id } userErrors { field message } } }',
      });

    const productId = createProductResponse.body.data.productCreate.product.id as string;

    const createMediaResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateMedia($productId: ID!, $media: [CreateMediaInput!]!) { productCreateMedia(productId: $productId, media: $media) { media { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } mediaUserErrors { field message } product { id media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } } } } }',
        variables: {
          productId,
          media: [
            {
              mediaContentType: 'IMAGE',
              originalSource: 'https://cdn.example.com/media-hat-front.jpg',
              alt: 'Front view',
            },
          ],
        },
      });

    expect(createMediaResponse.status).toBe(200);
    expect(createMediaResponse.body.data.productCreateMedia.mediaUserErrors).toEqual([]);
    expect(createMediaResponse.body.data.productCreateMedia.media).toEqual([
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/MediaImage\//),
        alt: 'Front view',
        mediaContentType: 'IMAGE',
        status: 'UPLOADED',
        preview: {
          image: {
            url: 'https://cdn.example.com/media-hat-front.jpg',
          },
        },
        image: {
          url: 'https://cdn.example.com/media-hat-front.jpg',
        },
      },
    ]);
    expect(createMediaResponse.body.data.productCreateMedia.product.media.nodes).toEqual(
      createMediaResponse.body.data.productCreateMedia.media,
    );

    const mediaId = createMediaResponse.body.data.productCreateMedia.media[0].id as string;

    const updateMediaResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateMedia($productId: ID!, $media: [UpdateMediaInput!]!) { productUpdateMedia(productId: $productId, media: $media) { media { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } mediaUserErrors { field message } } }',
        variables: {
          productId,
          media: [
            {
              id: mediaId,
              alt: 'Updated front view',
            },
          ],
        },
      });

    expect(updateMediaResponse.status).toBe(200);
    expect(updateMediaResponse.body.data.productUpdateMedia.mediaUserErrors).toEqual([]);
    expect(updateMediaResponse.body.data.productUpdateMedia.media).toEqual([
      {
        id: mediaId,
        alt: 'Updated front view',
        mediaContentType: 'IMAGE',
        status: 'UPLOADED',
        preview: {
          image: {
            url: 'https://cdn.example.com/media-hat-front.jpg',
          },
        },
        image: {
          url: 'https://cdn.example.com/media-hat-front.jpg',
        },
      },
    ]);

    const queryMediaResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query MediaDetail($id: ID!) { product(id: $id) { id media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } } } }',
        variables: { id: productId },
      });

    expect(queryMediaResponse.status).toBe(200);
    expect(queryMediaResponse.body.data.product.media.nodes).toEqual([
      {
        id: mediaId,
        alt: 'Updated front view',
        mediaContentType: 'IMAGE',
        status: 'UPLOADED',
        preview: {
          image: {
            url: 'https://cdn.example.com/media-hat-front.jpg',
          },
        },
        image: {
          url: 'https://cdn.example.com/media-hat-front.jpg',
        },
      },
    ]);

    const deleteMediaResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeleteMedia($productId: ID!, $mediaIds: [ID!]!) { productDeleteMedia(productId: $productId, mediaIds: $mediaIds) { deletedMediaIds deletedProductImageIds mediaUserErrors { field message } product { id media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } } } } }',
        variables: {
          productId,
          mediaIds: [mediaId],
        },
      });

    expect(deleteMediaResponse.status).toBe(200);
    expect(deleteMediaResponse.body.data.productDeleteMedia.mediaUserErrors).toEqual([]);
    expect(deleteMediaResponse.body.data.productDeleteMedia.deletedMediaIds).toEqual([mediaId]);
    expect(deleteMediaResponse.body.data.productDeleteMedia.deletedProductImageIds).toEqual([mediaId]);
    expect(deleteMediaResponse.body.data.productDeleteMedia.product.media.nodes).toEqual([]);

    const emptyMediaQuery = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query MediaAfterDelete($id: ID!) { product(id: $id) { id media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: { id: productId },
      });

    expect(emptyMediaQuery.status).toBe(200);
    expect(emptyMediaQuery.body.data.product.media).toEqual({
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages synchronous productSet creates with product options, variants, and metafields for immediate downstream reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('productSet should not hit upstream fetch');
    });

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const setResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateViaProductSet($input: ProductSetInput!) { productSet(input: $input) { product { id title handle status vendor productType tags options { id name position values optionValues { id name hasVariants } } variants(first: 10) { nodes { id title sku price inventoryQuantity selectedOptions { name value } inventoryItem { tracked requiresShipping } } } metafields(first: 10) { nodes { id namespace key type value } } } productSetOperation { id status } userErrors { field message } } }',
        variables: {
          input: {
            title: 'Set Snowboard',
            status: 'DRAFT',
            vendor: 'BURTON',
            productType: 'SNOWBOARD',
            tags: ['winter', 'featured'],
            productOptions: [
              {
                name: 'Color',
                position: 1,
                values: [{ name: 'Blue' }, { name: 'Black' }],
              },
            ],
            variants: [
              {
                optionValues: [{ optionName: 'Color', name: 'Blue' }],
                sku: 'SNOW-SET-BLUE',
                price: '79.99',
                inventoryQuantities: [{ quantity: 7 }],
                inventoryItem: { tracked: true, requiresShipping: true },
              },
              {
                optionValues: [{ optionName: 'Color', name: 'Black' }],
                sku: 'SNOW-SET-BLACK',
                price: '69.99',
                inventoryQuantities: [{ quantity: 3 }],
                inventoryItem: { tracked: false, requiresShipping: true },
              },
            ],
            metafields: [
              { namespace: 'custom', key: 'season', type: 'single_line_text_field', value: 'winter' },
            ],
          },
        },
      });

    expect(setResponse.status).toBe(200);
    expect(setResponse.body.data.productSet.userErrors).toEqual([]);
    expect(setResponse.body.data.productSet.productSetOperation).toBeNull();
    expect(setResponse.body.data.productSet.product).toMatchObject({
      title: 'Set Snowboard',
      handle: 'set-snowboard',
      status: 'DRAFT',
      vendor: 'BURTON',
      productType: 'SNOWBOARD',
      tags: ['winter', 'featured'],
    });
    expect(setResponse.body.data.productSet.product.options).toEqual([
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/ProductOption\//),
        name: 'Color',
        position: 1,
        values: ['Blue', 'Black'],
        optionValues: [
          {
            id: expect.stringMatching(/^gid:\/\/shopify\/ProductOptionValue\//),
            name: 'Blue',
            hasVariants: true,
          },
          {
            id: expect.stringMatching(/^gid:\/\/shopify\/ProductOptionValue\//),
            name: 'Black',
            hasVariants: true,
          },
        ],
      },
    ]);
    expect(setResponse.body.data.productSet.product.variants.nodes).toEqual([
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/ProductVariant\//),
        title: 'Blue',
        sku: 'SNOW-SET-BLUE',
        price: '79.99',
        inventoryQuantity: 7,
        selectedOptions: [{ name: 'Color', value: 'Blue' }],
        inventoryItem: { tracked: true, requiresShipping: true },
      },
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/ProductVariant\//),
        title: 'Black',
        sku: 'SNOW-SET-BLACK',
        price: '69.99',
        inventoryQuantity: 3,
        selectedOptions: [{ name: 'Color', value: 'Black' }],
        inventoryItem: { tracked: false, requiresShipping: true },
      },
    ]);
    expect(setResponse.body.data.productSet.product.metafields.nodes).toEqual([
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/Metafield\//),
        namespace: 'custom',
        key: 'season',
        type: 'single_line_text_field',
        value: 'winter',
      },
    ]);

    const productId = setResponse.body.data.productSet.product.id as string;
    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ProductSetReadback($id: ID!) { product(id: $id) { id title options { name values } variants(first: 10) { nodes { sku inventoryQuantity selectedOptions { name value } } } metafield(namespace: "custom", key: "season") { value } } total: productsCount(query: "sku:SNOW-SET-BLUE") { count precision } }',
        variables: { id: productId },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        product: {
          id: productId,
          title: 'Set Snowboard',
          options: [{ name: 'Color', values: ['Blue', 'Black'] }],
          variants: {
            nodes: [
              { sku: 'SNOW-SET-BLUE', inventoryQuantity: 7, selectedOptions: [{ name: 'Color', value: 'Blue' }] },
              { sku: 'SNOW-SET-BLACK', inventoryQuantity: 3, selectedOptions: [{ name: 'Color', value: 'Black' }] },
            ],
          },
          metafield: { value: 'winter' },
        },
        total: {
          count: 1,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages asynchronous productSet updates by handle with list-field replacement semantics', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('productSet should not hit upstream fetch');
    });
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/700',
        legacyResourceId: '700',
        title: 'Hybrid Board',
        handle: 'hybrid-board',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'BURTON',
        productType: 'SNOWBOARD',
        tags: ['winter'],
        totalInventory: 12,
        tracksInventory: true,
        descriptionHtml: '<p>Base board</p>',
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: 'Base SEO', description: 'Base SEO description' },
        category: null,
      },
    ]);
    store.replaceBaseOptionsForProduct('gid://shopify/Product/700', [
      {
        id: 'gid://shopify/ProductOption/7001',
        productId: 'gid://shopify/Product/700',
        name: 'Color',
        position: 1,
        optionValues: [
          { id: 'gid://shopify/ProductOptionValue/70011', name: 'Blue', hasVariants: true },
          { id: 'gid://shopify/ProductOptionValue/70012', name: 'Red', hasVariants: true },
        ],
      },
    ]);
    store.replaceBaseVariantsForProduct('gid://shopify/Product/700', [
      {
        id: 'gid://shopify/ProductVariant/70001',
        productId: 'gid://shopify/Product/700',
        title: 'Blue',
        sku: 'HYBRID-BLUE',
        barcode: null,
        price: '50.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 5,
        selectedOptions: [{ name: 'Color', value: 'Blue' }],
        inventoryItem: { id: 'gid://shopify/InventoryItem/70001', tracked: true, requiresShipping: true, measurement: null, countryCodeOfOrigin: null, provinceCodeOfOrigin: null, harmonizedSystemCode: null },
      },
      {
        id: 'gid://shopify/ProductVariant/70002',
        productId: 'gid://shopify/Product/700',
        title: 'Red',
        sku: 'HYBRID-RED',
        barcode: null,
        price: '55.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 7,
        selectedOptions: [{ name: 'Color', value: 'Red' }],
        inventoryItem: { id: 'gid://shopify/InventoryItem/70002', tracked: true, requiresShipping: true, measurement: null, countryCodeOfOrigin: null, provinceCodeOfOrigin: null, harmonizedSystemCode: null },
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/700', [
      { id: 'gid://shopify/Collection/1', productId: 'gid://shopify/Product/700', title: 'Winter', handle: 'winter' },
      { id: 'gid://shopify/Collection/2', productId: 'gid://shopify/Product/700', title: 'Sale', handle: 'sale' },
    ]);
    store.replaceBaseMetafieldsForProduct('gid://shopify/Product/700', [
      { id: 'gid://shopify/Metafield/7001', productId: 'gid://shopify/Product/700', namespace: 'custom', key: 'season', type: 'single_line_text_field', value: 'old' },
      { id: 'gid://shopify/Metafield/7002', productId: 'gid://shopify/Product/700', namespace: 'custom', key: 'material', type: 'single_line_text_field', value: 'wood' },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const setResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateViaProductSet($identifier: ProductSetIdentifiers, $input: ProductSetInput!, $synchronous: Boolean!) { productSet(identifier: $identifier, input: $input, synchronous: $synchronous) { product { id title } productSetOperation { id status userErrors { field message } } userErrors { field message } } }',
        variables: {
          synchronous: false,
          identifier: { handle: 'hybrid-board' },
          input: {
            title: 'Hybrid Board 2',
            tags: ['fresh-tag'],
            collections: ['gid://shopify/Collection/2'],
            productOptions: [
              {
                id: 'gid://shopify/ProductOption/7001',
                name: 'Color',
                position: 1,
                values: [{ name: 'Blue' }, { name: 'Green' }],
              },
            ],
            variants: [
              {
                id: 'gid://shopify/ProductVariant/70001',
                optionValues: [{ optionName: 'Color', name: 'Blue' }],
                sku: 'HYBRID-BLUE-2',
                price: '60.00',
                inventoryQuantities: [{ quantity: 9 }],
              },
            ],
            metafields: [
              { namespace: 'custom', key: 'season', type: 'single_line_text_field', value: 'new' },
            ],
          },
        },
      });

    expect(setResponse.status).toBe(200);
    expect(setResponse.body.data.productSet.userErrors).toEqual([]);
    expect(setResponse.body.data.productSet.product).toBeNull();
    expect(setResponse.body.data.productSet.productSetOperation).toEqual({
      id: expect.stringMatching(/^gid:\/\/shopify\/ProductSetOperation\//),
      status: 'CREATED',
      userErrors: [],
    });

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ProductSetAsyncReadback($id: ID!) { product(id: $id) { id title tags collections(first: 10) { nodes { id title handle } } options { name values optionValues { name hasVariants } } variants(first: 10) { nodes { id sku price inventoryQuantity selectedOptions { name value } } } metafields(first: 10) { nodes { namespace key value } } } blueCount: productsCount(query: "sku:HYBRID-BLUE-2") { count precision } redCount: productsCount(query: "sku:HYBRID-RED") { count precision } }',
        variables: { id: 'gid://shopify/Product/700' },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        product: {
          id: 'gid://shopify/Product/700',
          title: 'Hybrid Board 2',
          tags: ['fresh-tag'],
          collections: {
            nodes: [{ id: 'gid://shopify/Collection/2', title: 'Sale', handle: 'sale' }],
          },
          options: [
            {
              name: 'Color',
              values: ['Blue', 'Green'],
              optionValues: [
                { name: 'Blue', hasVariants: true },
                { name: 'Green', hasVariants: false },
              ],
            },
          ],
          variants: {
            nodes: [
              {
                id: 'gid://shopify/ProductVariant/70001',
                sku: 'HYBRID-BLUE-2',
                price: '60.00',
                inventoryQuantity: 9,
                selectedOptions: [{ name: 'Color', value: 'Blue' }],
              },
            ],
          },
          metafields: {
            nodes: [{ namespace: 'custom', key: 'season', value: 'new' }],
          },
        },
        blueCount: { count: 1, precision: 'EXACT' },
        redCount: { count: 0, precision: 'EXACT' },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('replaces staged productSet collections across repeated writes and keeps collection-derived reads aligned', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/710',
        legacyResourceId: '710',
        title: 'Collection Board',
        handle: 'collection-board',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'BURTON',
        productType: 'SNOWBOARD',
        tags: ['winter'],
        totalInventory: 4,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/710', [
      { id: 'gid://shopify/Collection/10', productId: 'gid://shopify/Product/710', title: 'Winter', handle: 'winter' },
      { id: 'gid://shopify/Collection/20', productId: 'gid://shopify/Product/710', title: 'Sale', handle: 'sale' },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();
    const productSetQuery =
      'mutation UpdateCollections($identifier: ProductSetIdentifiers, $input: ProductSetInput!) { productSet(identifier: $identifier, input: $input, synchronous: true) { product { id collections(first: 10) { nodes { id title handle } } } userErrors { field message } } }';

    const firstSetResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: productSetQuery,
        variables: {
          identifier: { id: 'gid://shopify/Product/710' },
          input: {
            collections: ['gid://shopify/Collection/20'],
          },
        },
      });

    expect(firstSetResponse.status).toBe(200);
    expect(firstSetResponse.body.data.productSet.userErrors).toEqual([]);
    expect(firstSetResponse.body.data.productSet.product.collections.nodes).toEqual([
      { id: 'gid://shopify/Collection/20', title: 'Sale', handle: 'sale' },
    ]);

    const secondSetResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: productSetQuery,
        variables: {
          identifier: { id: 'gid://shopify/Product/710' },
          input: {
            collections: ['gid://shopify/Collection/30'],
          },
        },
      });

    expect(secondSetResponse.status).toBe(200);
    expect(secondSetResponse.body.data.productSet.userErrors).toEqual([]);
    expect(secondSetResponse.body.data.productSet.product.collections.nodes).toEqual([
      { id: 'gid://shopify/Collection/30', title: '30', handle: '30' },
    ]);

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query CollectionReplacement($productId: ID!, $oldCollectionId: ID!, $newCollectionId: ID!) { product(id: $productId) { id collections(first: 10) { nodes { id title handle } } } oldCollection: collection(id: $oldCollectionId) { id title handle products(first: 10) { nodes { id title } } } newCollection: collection(id: $newCollectionId) { id title handle products(first: 10) { nodes { id title } } } collections(first: 10) { nodes { id title handle } } }',
        variables: {
          productId: 'gid://shopify/Product/710',
          oldCollectionId: 'gid://shopify/Collection/20',
          newCollectionId: 'gid://shopify/Collection/30',
        },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        product: {
          id: 'gid://shopify/Product/710',
          collections: {
            nodes: [{ id: 'gid://shopify/Collection/30', title: '30', handle: '30' }],
          },
        },
        oldCollection: null,
        newCollection: {
          id: 'gid://shopify/Collection/30',
          title: '30',
          handle: '30',
          products: {
            nodes: [{ id: 'gid://shopify/Product/710', title: 'Collection Board' }],
          },
        },
        collections: {
          nodes: [{ id: 'gid://shopify/Collection/30', title: '30', handle: '30' }],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages inventoryAdjustQuantities locally and keeps downstream product, variant, and count reads aligned', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/800',
        legacyResourceId: '800',
        title: 'Inventory Hoodie',
        handle: 'inventory-hoodie',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'ACME',
        productType: 'APPAREL',
        tags: ['inventory'],
        totalInventory: 12,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseVariantsForProduct('gid://shopify/Product/800', [
      {
        id: 'gid://shopify/ProductVariant/8001',
        productId: 'gid://shopify/Product/800',
        title: 'Blue / Small',
        sku: 'INV-BL-S',
        barcode: null,
        price: '40.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 5,
        selectedOptions: [
          { name: 'Color', value: 'Blue' },
          { name: 'Size', value: 'Small' },
        ],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8001',
          tracked: true,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
        },
      },
      {
        id: 'gid://shopify/ProductVariant/8002',
        productId: 'gid://shopify/Product/800',
        title: 'Blue / Medium',
        sku: 'INV-BL-M',
        barcode: null,
        price: '40.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 7,
        selectedOptions: [
          { name: 'Color', value: 'Blue' },
          { name: 'Size', value: 'Medium' },
        ],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8002',
          tracked: true,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
        },
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation AdjustInventory($input: InventoryAdjustQuantitiesInput!) { inventoryAdjustQuantities(input: $input) { inventoryAdjustmentGroup { createdAt reason referenceDocumentUri changes { name delta quantityAfterChange item { id } location { id } } } userErrors { field message } } }',
        variables: {
          input: {
            name: 'available',
            reason: 'correction',
            referenceDocumentUri: 'logistics://cycle-count/2026-04-15',
            changes: [
              {
                inventoryItemId: 'gid://shopify/InventoryItem/8001',
                locationId: 'gid://shopify/Location/1',
                delta: -2,
              },
              {
                inventoryItemId: 'gid://shopify/InventoryItem/8002',
                locationId: 'gid://shopify/Location/1',
                delta: 4,
              },
            ],
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.inventoryAdjustQuantities.userErrors).toEqual([]);
    expect(response.body.data.inventoryAdjustQuantities.inventoryAdjustmentGroup).toEqual({
      createdAt: expect.any(String),
      reason: 'correction',
      referenceDocumentUri: 'logistics://cycle-count/2026-04-15',
      changes: [
        {
          name: 'available',
          delta: -2,
          quantityAfterChange: 3,
          item: { id: 'gid://shopify/InventoryItem/8001' },
          location: { id: 'gid://shopify/Location/1' },
        },
        {
          name: 'available',
          delta: 4,
          quantityAfterChange: 11,
          item: { id: 'gid://shopify/InventoryItem/8002' },
          location: { id: 'gid://shopify/Location/1' },
        },
      ],
    });

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query InventoryAfterAdjust($productId: ID!, $variantId: ID!, $inventoryItemId: ID!) { product(id: $productId) { id totalInventory tracksInventory variants(first: 10) { nodes { id sku inventoryQuantity inventoryItem { id tracked } } } } variant: productVariant(id: $variantId) { id sku inventoryQuantity inventoryItem { id tracked } } stock: inventoryItem(id: $inventoryItemId) { id tracked variant { id inventoryQuantity product { id totalInventory } } } matching: products(first: 10, query: "inventory_total:>=14") { nodes { id totalInventory } } matchingCount: productsCount(query: "inventory_total:>=14") { count precision } }',
        variables: {
          productId: 'gid://shopify/Product/800',
          variantId: 'gid://shopify/ProductVariant/8001',
          inventoryItemId: 'gid://shopify/InventoryItem/8002',
        },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        product: {
          id: 'gid://shopify/Product/800',
          totalInventory: 14,
          tracksInventory: true,
          variants: {
            nodes: [
              {
                id: 'gid://shopify/ProductVariant/8001',
                sku: 'INV-BL-S',
                inventoryQuantity: 3,
                inventoryItem: { id: 'gid://shopify/InventoryItem/8001', tracked: true },
              },
              {
                id: 'gid://shopify/ProductVariant/8002',
                sku: 'INV-BL-M',
                inventoryQuantity: 11,
                inventoryItem: { id: 'gid://shopify/InventoryItem/8002', tracked: true },
              },
            ],
          },
        },
        variant: {
          id: 'gid://shopify/ProductVariant/8001',
          sku: 'INV-BL-S',
          inventoryQuantity: 3,
          inventoryItem: { id: 'gid://shopify/InventoryItem/8001', tracked: true },
        },
        stock: {
          id: 'gid://shopify/InventoryItem/8002',
          tracked: true,
          variant: {
            id: 'gid://shopify/ProductVariant/8002',
            inventoryQuantity: 11,
            product: {
              id: 'gid://shopify/Product/800',
              totalInventory: 14,
            },
          },
        },
        matching: {
          nodes: [{ id: 'gid://shopify/Product/800', totalInventory: 14 }],
        },
        matchingCount: { count: 1, precision: 'EXACT' },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns a user error for unknown inventory items without mutating downstream inventory reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/801',
        legacyResourceId: '801',
        title: 'Inventory Beanie',
        handle: 'inventory-beanie',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'ACME',
        productType: 'APPAREL',
        tags: ['inventory'],
        totalInventory: 6,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseVariantsForProduct('gid://shopify/Product/801', [
      {
        id: 'gid://shopify/ProductVariant/8011',
        productId: 'gid://shopify/Product/801',
        title: 'Default Title',
        sku: 'INV-BEANIE',
        barcode: null,
        price: '20.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 6,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8011',
          tracked: true,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
        },
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();
    const mutationResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation AdjustInventory($input: InventoryAdjustQuantitiesInput!) { inventoryAdjustQuantities(input: $input) { inventoryAdjustmentGroup { changes { delta } } userErrors { field message } } }',
        variables: {
          input: {
            name: 'available',
            reason: 'correction',
            changes: [
              {
                inventoryItemId: 'gid://shopify/InventoryItem/does-not-exist',
                locationId: 'gid://shopify/Location/1',
                delta: -3,
              },
            ],
          },
        },
      });

    expect(mutationResponse.status).toBe(200);
    expect(mutationResponse.body).toEqual({
      data: {
        inventoryAdjustQuantities: {
          inventoryAdjustmentGroup: null,
          userErrors: [{ field: ['input', 'changes', 'inventoryItemId'], message: 'Inventory item not found' }],
        },
      },
    });

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'query ($id: ID!) { product(id: $id) { id totalInventory variants(first: 10) { nodes { id inventoryQuantity } } } }',
        variables: { id: 'gid://shopify/Product/801' },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        product: {
          id: 'gid://shopify/Product/801',
          totalInventory: 6,
          variants: {
            nodes: [{ id: 'gid://shopify/ProductVariant/8011', inventoryQuantity: 6 }],
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
