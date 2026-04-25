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

describe('product query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves products edges and pageInfo from staged local state', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            products: {
              nodes: [],
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );

    const app = createApp(config).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Red Hat" }) { product { id } userErrors { field message } } }',
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Blue Hat" }) { product { id } userErrors { field message } } }',
    });

    const response = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 10) { edges { cursor node { id title handle } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
    });

    expect(response.status).toBe(200);
    expect(response.body.data.products.edges).toHaveLength(2);
    expect(response.body.data.products.edges[0]).toMatchObject({
      cursor: expect.stringMatching(/^cursor:gid:\/\/shopify\/Product\//),
      node: expect.objectContaining({ title: 'Blue Hat', handle: 'blue-hat' }),
    });
    expect(response.body.data.products.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: response.body.data.products.edges[0].cursor,
      endCursor: response.body.data.products.edges[1].cursor,
    });
  });

  it('returns null for a deleted product on direct product lookup', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Soon Gone" }) { product { id } userErrors { field message } } }',
    });

    const createdId = createResponse.body.data.productCreate.product.id as string;

    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation productDelete($input: ProductDeleteInput!) { productDelete(input: $input) { deletedProductId userErrors { field message } } }',
        variables: {
          input: { id: createdId },
        },
      });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'query ($id: ID!) { product(id: $id) { id title } }',
        variables: { id: createdId },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        product: null,
      },
    });
  });

  it('serves productsCount from staged local state in snapshot mode', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Nike Hat" }) { product { id } userErrors { field message } } }',
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Nike Shirt", status: DRAFT }) { product { id } userErrors { field message } } }',
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Adidas Hat" }) { product { id } userErrors { field message } } }',
    });

    const response = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { total: productsCount { count precision } nikeActive: productsCount(query: "title:Nike status:active") { count precision } }',
    });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        total: {
          count: 3,
          precision: 'EXACT',
        },
        nikeActive: {
          count: 1,
          precision: 'EXACT',
        },
      },
    });
  });

  it('overlays productsCount onto hydrated upstream catalog state after staged mutations', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          data: {
            productsCount: {
              count: 2,
              precision: 'EXACT',
            },
            products: {
              nodes: [
                {
                  id: 'gid://shopify/Product/1',
                  title: 'Nike Base Hat',
                  handle: 'nike-base-hat',
                  status: 'ACTIVE',
                  vendor: 'NIKE',
                  createdAt: '2024-01-01T00:00:00.000Z',
                  updatedAt: '2024-01-02T00:00:00.000Z',
                },
                {
                  id: 'gid://shopify/Product/2',
                  title: 'Adidas Base Shoe',
                  handle: 'adidas-base-shoe',
                  status: 'ACTIVE',
                  vendor: 'ADIDAS',
                  createdAt: '2024-01-03T00:00:00.000Z',
                  updatedAt: '2024-01-04T00:00:00.000Z',
                },
              ],
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/2", title: "Nike Base Shoe" }) { product { id title } userErrors { field message } } }',
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productDelete(input: { id: "gid://shopify/Product/1" }) { deletedProductId userErrors { field message } } }',
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Nike Draft Cap", status: DRAFT }) { product { id } userErrors { field message } } }',
    });

    const response = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { total: productsCount { count precision } nikeActive: productsCount(query: "title:Nike status:active") { count precision } drafts: productsCount(query: "status:draft") { count precision } }',
    });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        total: {
          count: 2,
          precision: 'EXACT',
        },
        nikeActive: {
          count: 1,
          precision: 'EXACT',
        },
        drafts: {
          count: 1,
          precision: 'EXACT',
        },
      },
    });
  });

  it('hydrates upstream product variants and options into staged product reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/1',
              title: 'Base Shirt',
              handle: 'base-shirt',
              status: 'ACTIVE',
              createdAt: '2024-01-02T00:00:00.000Z',
              updatedAt: '2024-01-03T00:00:00.000Z',
              options: [
                {
                  id: 'gid://shopify/ProductOption/100',
                  name: 'Size',
                  position: 1,
                  optionValues: [
                    {
                      id: 'gid://shopify/ProductOptionValue/1000',
                      name: 'Small',
                      hasVariants: true,
                    },
                    {
                      id: 'gid://shopify/ProductOptionValue/1001',
                      name: 'Medium',
                      hasVariants: true,
                    },
                  ],
                },
              ],
              variants: {
                nodes: [
                  {
                    id: 'gid://shopify/ProductVariant/10',
                    title: 'Small',
                  },
                  {
                    id: 'gid://shopify/ProductVariant/11',
                    title: 'Medium',
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

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/1", title: "Renamed Shirt" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id title options { id name position optionValues { id name hasVariants } } variants(first: 10) { edges { cursor node { id title } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: 'gid://shopify/Product/1' },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.options).toEqual([
      {
        id: 'gid://shopify/ProductOption/100',
        name: 'Size',
        position: 1,
        optionValues: [
          {
            id: 'gid://shopify/ProductOptionValue/1000',
            name: 'Small',
            hasVariants: true,
          },
          {
            id: 'gid://shopify/ProductOptionValue/1001',
            name: 'Medium',
            hasVariants: true,
          },
        ],
      },
    ]);
    expect(response.body.data.product.variants.edges).toEqual([
      {
        cursor: 'cursor:gid://shopify/ProductVariant/10',
        node: {
          id: 'gid://shopify/ProductVariant/10',
          title: 'Small',
        },
      },
      {
        cursor: 'cursor:gid://shopify/ProductVariant/11',
        node: {
          id: 'gid://shopify/ProductVariant/11',
          title: 'Medium',
        },
      },
    ]);
    expect(response.body.data.product.variants.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: 'cursor:gid://shopify/ProductVariant/10',
      endCursor: 'cursor:gid://shopify/ProductVariant/11',
    });
  });

  it('hydrates variant merchandising and inventory fields from upstream conformance-shaped reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/8397256720617',
              title: 'Converse Shoe',
              handle: 'converse-shoe',
              status: 'ACTIVE',
              createdAt: '2024-01-02T00:00:00.000Z',
              updatedAt: '2024-01-03T00:00:00.000Z',
              variants: {
                edges: [
                  {
                    node: {
                      id: 'gid://shopify/ProductVariant/46789263425769',
                      title: '5 / black',
                      sku: 'C-03-black-5',
                      barcode: null,
                      price: '70.00',
                      compareAtPrice: null,
                      taxable: true,
                      inventoryPolicy: 'DENY',
                      inventoryQuantity: 14,
                      selectedOptions: [
                        { name: 'Size', value: '5' },
                        { name: 'Color', value: 'black' },
                      ],
                      inventoryItem: {
                        id: 'gid://shopify/InventoryItem/48886350676201',
                        tracked: true,
                        requiresShipping: true,
                      },
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

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397256720617", title: "Converse Shoe Renamed" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id title variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } } }',
        variables: { id: 'gid://shopify/Product/8397256720617' },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.title).toBe('Converse Shoe Renamed');
    expect(response.body.data.product.variants.nodes).toEqual([
      {
        id: 'gid://shopify/ProductVariant/46789263425769',
        title: '5 / black',
        sku: 'C-03-black-5',
        barcode: null,
        price: '70.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 14,
        selectedOptions: [
          { name: 'Size', value: '5' },
          { name: 'Color', value: 'black' },
        ],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/48886350676201',
          tracked: true,
          requiresShipping: true,
        },
      },
    ]);
  });

  it('hydrates rich inventory item origin and measurement fields from upstream conformance-shaped reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/8397256720617',
              title: 'Converse Shoe',
              handle: 'converse-shoe',
              status: 'ACTIVE',
              createdAt: '2024-01-02T00:00:00.000Z',
              updatedAt: '2024-01-03T00:00:00.000Z',
              variants: {
                edges: [
                  {
                    node: {
                      id: 'gid://shopify/ProductVariant/46789263425769',
                      title: '5 / black',
                      inventoryItem: {
                        id: 'gid://shopify/InventoryItem/48886350676201',
                        tracked: true,
                        requiresShipping: true,
                        measurement: {
                          weight: {
                            unit: 'KILOGRAMS',
                            value: 0,
                          },
                        },
                        countryCodeOfOrigin: null,
                        provinceCodeOfOrigin: null,
                        harmonizedSystemCode: null,
                      },
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

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397256720617", title: "Converse Shoe Renamed" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id title variants(first: 10) { nodes { id title inventoryItem { id tracked requiresShipping measurement { weight { unit value } } countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode } } } } }',
        variables: { id: 'gid://shopify/Product/8397256720617' },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.title).toBe('Converse Shoe Renamed');
    expect(response.body.data.product.variants.nodes).toEqual([
      {
        id: 'gid://shopify/ProductVariant/46789263425769',
        title: '5 / black',
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/48886350676201',
          tracked: true,
          requiresShipping: true,
          measurement: {
            weight: {
              unit: 'KILOGRAMS',
              value: 0,
            },
          },
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
        },
      },
    ]);
  });

  it('paginates product variants connections with first/after and preserves selection-aware edges and pageInfo', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/8397256720617',
              title: 'Converse Shoe',
              handle: 'converse-shoe',
              status: 'ACTIVE',
              createdAt: '2024-01-02T00:00:00.000Z',
              updatedAt: '2024-01-03T00:00:00.000Z',
              variants: {
                edges: [
                  { node: { id: 'gid://shopify/ProductVariant/46789263425769', title: '5 / black' } },
                  { node: { id: 'gid://shopify/ProductVariant/46789263458537', title: '6 / black' } },
                  { node: { id: 'gid://shopify/ProductVariant/46789263491305', title: '7 / black' } },
                ],
              },
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397256720617", title: "Converse Shoe Renamed" }) { product { id title } userErrors { field message } } }',
    });

    const firstPageResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id variants(first: 2) { edges { node { id title } } pageInfo { hasNextPage endCursor } } } }',
        variables: { id: 'gid://shopify/Product/8397256720617' },
      });

    expect(firstPageResponse.status).toBe(200);
    expect(firstPageResponse.body.data.product.variants).toEqual({
      edges: [
        {
          node: {
            id: 'gid://shopify/ProductVariant/46789263425769',
            title: '5 / black',
          },
        },
        {
          node: {
            id: 'gid://shopify/ProductVariant/46789263458537',
            title: '6 / black',
          },
        },
      ],
      pageInfo: {
        hasNextPage: true,
        endCursor: 'cursor:gid://shopify/ProductVariant/46789263458537',
      },
    });
    expect(firstPageResponse.body.data.product.variants.edges[0]).not.toHaveProperty('cursor');

    const secondPageResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!, $after: String!) { product(id: $id) { id variants(first: 2, after: $after) { nodes { id title } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: {
          id: 'gid://shopify/Product/8397256720617',
          after: 'cursor:gid://shopify/ProductVariant/46789263458537',
        },
      });

    expect(secondPageResponse.status).toBe(200);
    expect(secondPageResponse.body.data.product.variants).toEqual({
      nodes: [
        {
          id: 'gid://shopify/ProductVariant/46789263491305',
          title: '7 / black',
        },
      ],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: true,
        startCursor: 'cursor:gid://shopify/ProductVariant/46789263491305',
        endCursor: 'cursor:gid://shopify/ProductVariant/46789263491305',
      },
    });
  });

  it('hydrates upstream product collections into staged product reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/8397256720617',
              title: 'Converse Shoe',
              handle: 'converse-shoe',
              status: 'ACTIVE',
              createdAt: '2024-01-02T00:00:00.000Z',
              updatedAt: '2024-01-03T00:00:00.000Z',
              collections: {
                edges: [
                  {
                    node: {
                      id: 'gid://shopify/Collection/429826244841',
                      title: 'CONVERSE',
                      handle: 'converse',
                    },
                  },
                  {
                    node: {
                      id: 'gid://shopify/Collection/429826605289',
                      title: 'KID',
                      handle: 'kid',
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

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397256720617", title: "Converse Shoe Renamed" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id title collections(first: 5) { edges { node { id title handle } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: 'gid://shopify/Product/8397256720617' },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.title).toBe('Converse Shoe Renamed');
    expect(response.body.data.product.collections.edges).toEqual([
      {
        node: {
          id: 'gid://shopify/Collection/429826244841',
          title: 'CONVERSE',
          handle: 'converse',
        },
      },
      {
        node: {
          id: 'gid://shopify/Collection/429826605289',
          title: 'KID',
          handle: 'kid',
        },
      },
    ]);
    expect(response.body.data.product.collections.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: 'cursor:gid://shopify/Collection/429826244841',
      endCursor: 'cursor:gid://shopify/Collection/429826605289',
    });
  });

  it('returns an empty collections connection for staged products without memberships', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { product: null } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(config).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Collectionless Hat" }) { product { id } userErrors { field message } } }',
    });

    const createdId = createResponse.body.data.productCreate.product.id as string;

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id collections(first: 5) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: createdId },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.collections).toEqual({
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    });
  });

  it('paginates product collections connections with first/after', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/8397256720617',
              title: 'Converse Shoe',
              handle: 'converse-shoe',
              status: 'ACTIVE',
              createdAt: '2024-01-02T00:00:00.000Z',
              updatedAt: '2024-01-03T00:00:00.000Z',
              collections: {
                edges: [
                  { node: { id: 'gid://shopify/Collection/429826244841', title: 'CONVERSE', handle: 'converse' } },
                  { node: { id: 'gid://shopify/Collection/429826605289', title: 'KID', handle: 'kid' } },
                  { node: { id: 'gid://shopify/Collection/429826900000', title: 'SALE', handle: 'sale' } },
                ],
              },
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397256720617", title: "Converse Shoe Renamed" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!, $after: String!) { product(id: $id) { id collections(first: 1, after: $after) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: {
          id: 'gid://shopify/Product/8397256720617',
          after: 'cursor:gid://shopify/Collection/429826244841',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.collections).toEqual({
      nodes: [
        {
          id: 'gid://shopify/Collection/429826605289',
          title: 'KID',
          handle: 'kid',
        },
      ],
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: true,
        startCursor: 'cursor:gid://shopify/Collection/429826605289',
        endCursor: 'cursor:gid://shopify/Collection/429826605289',
      },
    });
  });

  it('hydrates upstream product media into staged product reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/8397256720617',
              title: 'Converse Shoe',
              handle: 'converse-shoe',
              status: 'ACTIVE',
              createdAt: '2024-01-02T00:00:00.000Z',
              updatedAt: '2024-01-03T00:00:00.000Z',
              media: {
                edges: [
                  {
                    node: {
                      mediaContentType: 'IMAGE',
                      alt: '',
                      preview: {
                        image: {
                          url: 'https://cdn.shopify.com/media-1.jpg',
                        },
                      },
                    },
                  },
                  {
                    node: {
                      mediaContentType: 'IMAGE',
                      alt: 'Side angle',
                      preview: {
                        image: {
                          url: 'https://cdn.shopify.com/media-2.jpg',
                        },
                      },
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

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397256720617", title: "Converse Shoe Renamed" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id title media(first: 10) { edges { node { mediaContentType alt preview { image { url } } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: 'gid://shopify/Product/8397256720617' },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.title).toBe('Converse Shoe Renamed');
    expect(response.body.data.product.media.edges).toEqual([
      {
        node: {
          mediaContentType: 'IMAGE',
          alt: '',
          preview: {
            image: {
              url: 'https://cdn.shopify.com/media-1.jpg',
            },
          },
        },
      },
      {
        node: {
          mediaContentType: 'IMAGE',
          alt: 'Side angle',
          preview: {
            image: {
              url: 'https://cdn.shopify.com/media-2.jpg',
            },
          },
        },
      },
    ]);
    expect(response.body.data.product.media.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: 'cursor:gid://shopify/Product/8397256720617:media:0',
      endCursor: 'cursor:gid://shopify/Product/8397256720617:media:1',
    });
  });

  it('returns an empty media connection for staged products without media', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { product: null } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(config).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Imageless Hat" }) { product { id } userErrors { field message } } }',
    });

    const createdId = createResponse.body.data.productCreate.product.id as string;

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id media(first: 5) { nodes { mediaContentType alt preview { image { url } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: createdId },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.media).toEqual({
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    });
  });

  it('returns an empty product images connection for staged products without images', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { product: null } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(config).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Imageless Hat" }) { product { id } userErrors { field message } } }',
    });

    const createdId = createResponse.body.data.productCreate.product.id as string;

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id images(first: 5) { nodes { id altText url width height } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: createdId },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.images).toEqual({
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    });
  });

  it('hydrates upstream product images into compatibility image reads after local overlays', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/8397256720617',
              title: 'Converse Shoe',
              handle: 'converse-shoe',
              status: 'ACTIVE',
              createdAt: '2024-01-02T00:00:00.000Z',
              updatedAt: '2024-01-03T00:00:00.000Z',
              images: {
                edges: [
                  {
                    node: {
                      id: 'gid://shopify/ProductImage/111',
                      altText: 'Front angle',
                      url: 'https://cdn.shopify.com/product-image-1.jpg',
                      width: 1200,
                      height: 900,
                    },
                  },
                  {
                    node: {
                      id: 'gid://shopify/ProductImage/112',
                      altText: null,
                      originalSrc: 'https://cdn.shopify.com/product-image-2.jpg',
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

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397256720617", title: "Converse Shoe Renamed" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id title images(first: 10) { edges { cursor node { __typename id altText url originalSrc transformedSrc width height } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: 'gid://shopify/Product/8397256720617' },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.title).toBe('Converse Shoe Renamed');
    expect(response.body.data.product.images).toEqual({
      edges: [
        {
          cursor: 'cursor:gid://shopify/Product/8397256720617:media:0',
          node: {
            __typename: 'Image',
            id: 'gid://shopify/ProductImage/111',
            altText: 'Front angle',
            url: 'https://cdn.shopify.com/product-image-1.jpg',
            originalSrc: 'https://cdn.shopify.com/product-image-1.jpg',
            transformedSrc: 'https://cdn.shopify.com/product-image-1.jpg',
            width: 1200,
            height: 900,
          },
        },
        {
          cursor: 'cursor:gid://shopify/Product/8397256720617:media:1',
          node: {
            __typename: 'Image',
            id: 'gid://shopify/ProductImage/112',
            altText: null,
            url: 'https://cdn.shopify.com/product-image-2.jpg',
            originalSrc: 'https://cdn.shopify.com/product-image-2.jpg',
            transformedSrc: 'https://cdn.shopify.com/product-image-2.jpg',
            width: null,
            height: null,
          },
        },
      ],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/Product/8397256720617:media:0',
        endCursor: 'cursor:gid://shopify/Product/8397256720617:media:1',
      },
    });
  });

  it('serializes ready staged product image media through the older images connection', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Image Hat" }) { product { id } userErrors { field message } } }',
    });

    const productId = createResponse.body.data.productCreate.product.id as string;

    const createMediaResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateMedia($productId: ID!, $media: [CreateMediaInput!]!) { productCreateMedia(productId: $productId, media: $media) { media { id } mediaUserErrors { field message } } }',
        variables: {
          productId,
          media: [
            {
              mediaContentType: 'IMAGE',
              originalSource: 'https://cdn.example.com/staged-hat-front.jpg',
              alt: 'Front',
            },
          ],
        },
      });

    expect(createMediaResponse.body.data.productCreateMedia.mediaUserErrors).toEqual([]);

    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query PromoteProcessing($id: ID!) { product(id: $id) { id media(first: 10) { nodes { status preview { image { url } } } } } }',
        variables: { id: productId },
      });

    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query PromoteReady($id: ID!) { product(id: $id) { id media(first: 10) { nodes { status preview { image { url } } } } } }',
        variables: { id: productId },
      });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ProductImages($id: ID!) { product(id: $id) { id images(first: 10) { nodes { __typename id altText url } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: productId },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.images).toEqual({
      nodes: [
        {
          __typename: 'Image',
          id: expect.stringMatching(/^gid:\/\/shopify\/ProductImage\//),
          altText: 'Front',
          url: 'https://cdn.example.com/staged-hat-front.jpg',
        },
      ],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: `cursor:${productId}:media:0`,
        endCursor: `cursor:${productId}:media:0`,
      },
    });
  });

  it('paginates product media connections with first/after', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/8397256720617',
              title: 'Converse Shoe',
              handle: 'converse-shoe',
              status: 'ACTIVE',
              createdAt: '2024-01-02T00:00:00.000Z',
              updatedAt: '2024-01-03T00:00:00.000Z',
              media: {
                edges: [
                  {
                    node: {
                      mediaContentType: 'IMAGE',
                      alt: '',
                      preview: { image: { url: 'https://cdn.shopify.com/media-1.jpg' } },
                    },
                  },
                  {
                    node: {
                      mediaContentType: 'IMAGE',
                      alt: 'Side angle',
                      preview: { image: { url: 'https://cdn.shopify.com/media-2.jpg' } },
                    },
                  },
                  {
                    node: {
                      mediaContentType: 'IMAGE',
                      alt: 'Back angle',
                      preview: { image: { url: 'https://cdn.shopify.com/media-3.jpg' } },
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

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397256720617", title: "Converse Shoe Renamed" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!, $after: String!) { product(id: $id) { id media(first: 1, after: $after) { nodes { mediaContentType alt preview { image { url } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: {
          id: 'gid://shopify/Product/8397256720617',
          after: 'cursor:gid://shopify/Product/8397256720617:media:0',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.media).toEqual({
      nodes: [
        {
          mediaContentType: 'IMAGE',
          alt: 'Side angle',
          preview: {
            image: {
              url: 'https://cdn.shopify.com/media-2.jpg',
            },
          },
        },
      ],
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: true,
        startCursor: 'cursor:gid://shopify/Product/8397256720617:media:1',
        endCursor: 'cursor:gid://shopify/Product/8397256720617:media:1',
      },
    });
  });

  it('serializes product media typenames, edge cursors, and MediaImage fragments', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/8397256720617',
              title: 'Converse Shoe',
              handle: 'converse-shoe',
              status: 'ACTIVE',
              createdAt: '2024-01-02T00:00:00.000Z',
              updatedAt: '2024-01-03T00:00:00.000Z',
              media: {
                edges: [
                  {
                    node: {
                      id: 'gid://shopify/MediaImage/111',
                      mediaContentType: 'IMAGE',
                      alt: 'Front angle',
                      status: 'READY',
                      preview: { image: { url: 'https://cdn.shopify.com/media-1.jpg' } },
                      image: { url: 'https://cdn.shopify.com/media-1-large.jpg' },
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

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397256720617", title: "Converse Shoe Renamed" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id media(first: 1) { edges { cursor node { __typename id mediaContentType alt status preview { image { url } } ... on MediaImage { image { url } } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: 'gid://shopify/Product/8397256720617' },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.media).toEqual({
      edges: [
        {
          cursor: 'cursor:gid://shopify/Product/8397256720617:media:0',
          node: {
            __typename: 'MediaImage',
            id: 'gid://shopify/MediaImage/111',
            mediaContentType: 'IMAGE',
            alt: 'Front angle',
            status: 'READY',
            preview: {
              image: {
                url: 'https://cdn.shopify.com/media-1.jpg',
              },
            },
            image: {
              url: 'https://cdn.shopify.com/media-1-large.jpg',
            },
          },
        },
      ],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/Product/8397256720617:media:0',
        endCursor: 'cursor:gid://shopify/Product/8397256720617:media:0',
      },
    });
  });

  it('hydrates upstream product metafield reads into staged product queries', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/8397256720617',
              title: 'Converse Shoe',
              handle: 'converse-shoe',
              status: 'ACTIVE',
              createdAt: '2024-01-02T00:00:00.000Z',
              updatedAt: '2024-01-03T00:00:00.000Z',
              metafield: {
                id: 'gid://shopify/Metafield/9001',
                namespace: 'custom',
                key: 'material',
                type: 'single_line_text_field',
                value: 'Canvas',
                compareDigest: 'compare-digest-9001',
                jsonValue: 'Canvas',
                createdAt: '2024-01-02T00:00:00Z',
                updatedAt: '2024-01-03T00:00:00Z',
                ownerType: 'PRODUCT',
              },
              metafields: {
                edges: [
                  {
                    node: {
                      id: 'gid://shopify/Metafield/9001',
                      namespace: 'custom',
                      key: 'material',
                      type: 'single_line_text_field',
                      value: 'Canvas',
                      compareDigest: 'compare-digest-9001',
                      jsonValue: 'Canvas',
                      createdAt: '2024-01-02T00:00:00Z',
                      updatedAt: '2024-01-03T00:00:00Z',
                      ownerType: 'PRODUCT',
                    },
                  },
                  {
                    node: {
                      id: 'gid://shopify/Metafield/9002',
                      namespace: 'details',
                      key: 'origin',
                      type: 'single_line_text_field',
                      value: 'VN',
                      compareDigest: 'compare-digest-9002',
                      jsonValue: 'VN',
                      createdAt: '2024-01-04T00:00:00Z',
                      updatedAt: '2024-01-05T00:00:00Z',
                      ownerType: 'PRODUCT',
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

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397256720617", title: "Converse Shoe Renamed" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id title primarySpec: metafield(namespace: "custom", key: "material") { id namespace key type value compareDigest jsonValue createdAt updatedAt ownerType definition { id name } } metafields(first: 10) { edges { cursor node { id namespace key type value compareDigest jsonValue createdAt updatedAt ownerType } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: 'gid://shopify/Product/8397256720617' },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.title).toBe('Converse Shoe Renamed');
    expect(response.body.data.product.primarySpec).toEqual({
      id: 'gid://shopify/Metafield/9001',
      namespace: 'custom',
      key: 'material',
      type: 'single_line_text_field',
      value: 'Canvas',
      compareDigest: 'compare-digest-9001',
      jsonValue: 'Canvas',
      createdAt: '2024-01-02T00:00:00Z',
      updatedAt: '2024-01-03T00:00:00Z',
      ownerType: 'PRODUCT',
      definition: null,
    });
    expect(response.body.data.product.metafields.edges).toEqual([
      {
        cursor: 'cursor:gid://shopify/Metafield/9001',
        node: {
          id: 'gid://shopify/Metafield/9001',
          namespace: 'custom',
          key: 'material',
          type: 'single_line_text_field',
          value: 'Canvas',
          compareDigest: 'compare-digest-9001',
          jsonValue: 'Canvas',
          createdAt: '2024-01-02T00:00:00Z',
          updatedAt: '2024-01-03T00:00:00Z',
          ownerType: 'PRODUCT',
        },
      },
      {
        cursor: 'cursor:gid://shopify/Metafield/9002',
        node: {
          id: 'gid://shopify/Metafield/9002',
          namespace: 'details',
          key: 'origin',
          type: 'single_line_text_field',
          value: 'VN',
          compareDigest: 'compare-digest-9002',
          jsonValue: 'VN',
          createdAt: '2024-01-04T00:00:00Z',
          updatedAt: '2024-01-05T00:00:00Z',
          ownerType: 'PRODUCT',
        },
      },
    ]);
    expect(response.body.data.product.metafields.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: 'cursor:gid://shopify/Metafield/9001',
      endCursor: 'cursor:gid://shopify/Metafield/9002',
    });
  });

  it('serializes staged product metafield read fields through singular and nodes selections', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Canvas Sneaker" }) { product { id } userErrors { field message } } }',
    });
    const productId = createResponse.body.data.productCreate.product.id as string;

    await request(app)
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
              key: 'dimensions',
              type: 'json',
              value: '{"height":12,"width":8}',
            },
          ],
        },
      });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { material: metafield(namespace: "custom", key: "material") { id namespace key type value compareDigest jsonValue createdAt updatedAt ownerType definition { id } } metafields(first: 2) { nodes { id namespace key type value compareDigest jsonValue createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: productId },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.material).toMatchObject({
      namespace: 'custom',
      key: 'material',
      type: 'single_line_text_field',
      value: 'Canvas',
      compareDigest: expect.stringMatching(/^draft:/),
      jsonValue: 'Canvas',
      ownerType: 'PRODUCT',
      definition: null,
    });
    expect(response.body.data.product.material.createdAt).toEqual(response.body.data.product.material.updatedAt);
    expect(response.body.data.product.metafields.nodes).toEqual([
      expect.objectContaining({
        namespace: 'custom',
        key: 'material',
        type: 'single_line_text_field',
        value: 'Canvas',
        compareDigest: expect.stringMatching(/^draft:/),
        jsonValue: 'Canvas',
        createdAt: response.body.data.product.material.createdAt,
        updatedAt: response.body.data.product.material.updatedAt,
        ownerType: 'PRODUCT',
      }),
      expect.objectContaining({
        namespace: 'details',
        key: 'dimensions',
        type: 'json',
        value: '{"height":12,"width":8}',
        compareDigest: expect.stringMatching(/^draft:/),
        jsonValue: { height: 12, width: 8 },
        ownerType: 'PRODUCT',
      }),
    ]);
    expect(response.body.data.product.metafields.pageInfo).toMatchObject({
      hasNextPage: false,
      hasPreviousPage: false,
    });
  });

  it('returns null and an empty metafields connection for staged products without metafields', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { product: null } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(config).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Metafieldless Hat" }) { product { id } userErrors { field message } } }',
    });

    const createdId = createResponse.body.data.productCreate.product.id as string;

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id metafield(namespace: "custom", key: "material") { id namespace key type value } metafields(first: 5) { nodes { id namespace key type value } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: createdId },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.metafield).toBeNull();
    expect(response.body.data.product.metafields).toEqual({
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    });
  });

  it('paginates product metafields connections with first/after', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/8397256720617',
              title: 'Converse Shoe',
              handle: 'converse-shoe',
              status: 'ACTIVE',
              createdAt: '2024-01-02T00:00:00.000Z',
              updatedAt: '2024-01-03T00:00:00.000Z',
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
                  {
                    node: {
                      id: 'gid://shopify/Metafield/9003',
                      namespace: 'details',
                      key: 'season',
                      type: 'single_line_text_field',
                      value: 'SS26',
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

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397256720617", title: "Converse Shoe Renamed" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!, $after: String!) { product(id: $id) { id metafields(first: 1, after: $after) { edges { cursor node { id namespace key value } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: {
          id: 'gid://shopify/Product/8397256720617',
          after: 'cursor:gid://shopify/Metafield/9001',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product.metafields).toEqual({
      edges: [
        {
          cursor: 'cursor:gid://shopify/Metafield/9002',
          node: {
            id: 'gid://shopify/Metafield/9002',
            namespace: 'details',
            key: 'origin',
            value: 'VN',
          },
        },
      ],
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: true,
        startCursor: 'cursor:gid://shopify/Metafield/9002',
        endCursor: 'cursor:gid://shopify/Metafield/9002',
      },
    });
  });

  it('hydrates upstream products search edges and applies vendor/status filtering with title sorting during overlay reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            products: {
              edges: [
                {
                  node: {
                    id: 'gid://shopify/Product/8397255999721',
                    title: 'NIKE | CRACKLE PRINT TB TEE',
                    handle: 'nike-crackle-print-tb-tee',
                    status: 'ACTIVE',
                    vendor: 'NIKE',
                    totalInventory: 16,
                    createdAt: '2024-03-14T01:53:01Z',
                    updatedAt: '2026-03-25T10:00:00Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8397257375977',
                    title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
                    handle: 'nike-swoosh-pro-flat-peak-cap',
                    status: 'ACTIVE',
                    vendor: 'NIKE',
                    totalInventory: 20,
                    createdAt: '2024-03-14T01:53:33Z',
                    updatedAt: '2026-03-25T16:49:35Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8397257081065',
                    title: 'VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE',
                    handle: 'vans-apparel-and-accessories-classic-super-no-show-socks-3-pack-white',
                    status: 'ACTIVE',
                    vendor: 'VANS',
                    totalInventory: 11,
                    createdAt: '2024-03-14T01:53:17Z',
                    updatedAt: '2026-03-25T14:27:56Z',
                  },
                },
              ],
              pageInfo: {
                hasNextPage: true,
              },
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397257375977", title: "AARDVARK NIKE CAP" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 5, query: "vendor:NIKE status:active", sortKey: TITLE) { edges { node { id title vendor status totalInventory } } pageInfo { hasNextPage } } }',
    });

    expect(response.status).toBe(200);
    expect(response.body.data.products.edges).toEqual([
      {
        node: {
          id: 'gid://shopify/Product/8397257375977',
          title: 'AARDVARK NIKE CAP',
          vendor: 'NIKE',
          status: 'ACTIVE',
          totalInventory: 20,
        },
      },
      {
        node: {
          id: 'gid://shopify/Product/8397255999721',
          title: 'NIKE | CRACKLE PRINT TB TEE',
          vendor: 'NIKE',
          status: 'ACTIVE',
          totalInventory: 16,
        },
      },
    ]);
    expect(response.body.data.products.pageInfo).toEqual({
      hasNextPage: false,
    });
  });

  it('filters products by inventory_total query terms and sorts by INVENTORY_TOTAL during overlay reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            products: {
              edges: [
                {
                  node: {
                    id: 'gid://shopify/Product/8971371708649',
                    title: 'Test Product - 8201',
                    handle: 'test-product-8201',
                    status: 'ACTIVE',
                    vendor: 'very-big-test-store',
                    totalInventory: 0,
                    createdAt: '2024-04-01T00:00:00Z',
                    updatedAt: '2026-03-25T08:00:00Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8971371741417',
                    title: 'Test Product - 6726',
                    handle: 'test-product-6726',
                    status: 'ACTIVE',
                    vendor: 'very-big-test-store',
                    totalInventory: 0,
                    createdAt: '2024-04-01T00:01:00Z',
                    updatedAt: '2026-03-25T08:01:00Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8971371749999',
                    title: 'Test Product - 9900',
                    handle: 'test-product-9900',
                    status: 'ACTIVE',
                    vendor: 'very-big-test-store',
                    totalInventory: 7,
                    createdAt: '2024-04-01T00:02:00Z',
                    updatedAt: '2026-03-25T08:02:00Z',
                  },
                },
              ],
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8971371741417", title: "Test Product - 6726 (Renamed)" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 5, query: "inventory_total:<=5 status:active", sortKey: INVENTORY_TOTAL) { edges { node { id title totalInventory } } } }',
    });

    expect(response.status).toBe(200);
    expect(response.body.data.products.edges).toEqual([
      {
        node: {
          id: 'gid://shopify/Product/8971371708649',
          title: 'Test Product - 8201',
          totalInventory: 0,
        },
      },
      {
        node: {
          id: 'gid://shopify/Product/8971371741417',
          title: 'Test Product - 6726 (Renamed)',
          totalInventory: 0,
        },
      },
    ]);
  });

  it('hydrates richer catalog merchandising fields into products overlay reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            products: {
              edges: [
                {
                  cursor: 'cursor-1',
                  node: {
                    id: 'gid://shopify/Product/8397256720617',
                    legacyResourceId: '8397256720617',
                    title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
                    handle: 'converse-toddler-chuck-taylor-all-star-axel-mid',
                    status: 'ACTIVE',
                    vendor: 'CONVERSE',
                    productType: 'SHOES',
                    tags: ['converse', 'egnition-sample-data', 'kid'],
                    totalInventory: 45,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:02Z',
                    updatedAt: '2026-03-25T19:35:13Z',
                  },
                },
              ],
              pageInfo: {
                hasNextPage: true,
                endCursor: 'cursor-1',
              },
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397256720617", title: "Renamed Converse" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 5) { edges { node { id legacyResourceId title vendor productType tags totalInventory tracksInventory } } pageInfo { hasNextPage endCursor } } }',
    });

    expect(response.status).toBe(200);
    expect(response.body.data.products.edges).toEqual([
      {
        node: {
          id: 'gid://shopify/Product/8397256720617',
          legacyResourceId: '8397256720617',
          title: 'Renamed Converse',
          vendor: 'CONVERSE',
          productType: 'SHOES',
          tags: ['converse', 'egnition-sample-data', 'kid'],
          totalInventory: 45,
          tracksInventory: true,
        },
      },
    ]);
    expect(response.body.data.products.pageInfo).toEqual({
      hasNextPage: false,
      endCursor: 'cursor:gid://shopify/Product/8397256720617',
    });
  });

  it('filters products by title, handle, tag, and product_type terms and supports reverse UPDATED_AT sorting', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(
      async () =>
        new Response(
          JSON.stringify({
            data: {
              products: {
                edges: [
                  {
                    node: {
                      id: 'gid://shopify/Product/8397255999721',
                      legacyResourceId: '8397255999721',
                      title: 'NIKE | CRACKLE PRINT TB TEE',
                      handle: 'nike-crackle-print-tb-tee',
                      status: 'ACTIVE',
                      vendor: 'NIKE',
                      productType: 'TOPS',
                      tags: ['nike', 'egnition-sample-data', 'tee'],
                      totalInventory: 16,
                      tracksInventory: true,
                      createdAt: '2024-03-14T01:53:01Z',
                      updatedAt: '2026-03-25T10:00:00Z',
                    },
                  },
                  {
                    node: {
                      id: 'gid://shopify/Product/8397257375977',
                      legacyResourceId: '8397257375977',
                      title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
                      handle: 'nike-swoosh-pro-flat-peak-cap',
                      status: 'ACTIVE',
                      vendor: 'NIKE',
                      productType: 'ACCESSORIES',
                      tags: ['cap', 'egnition-sample-data', 'nike'],
                      totalInventory: 20,
                      tracksInventory: true,
                      createdAt: '2024-03-14T01:53:33Z',
                      updatedAt: '2026-03-25T16:49:35Z',
                    },
                  },
                  {
                    node: {
                      id: 'gid://shopify/Product/8397257081065',
                      legacyResourceId: '8397257081065',
                      title: 'VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE',
                      handle: 'vans-apparel-and-accessories-classic-super-no-show-socks-3-pack-white',
                      status: 'ACTIVE',
                      vendor: 'VANS',
                      productType: 'ACCESSORIES',
                      tags: ['egnition-sample-data', 'unisex', 'vans'],
                      totalInventory: 11,
                      tracksInventory: true,
                      createdAt: '2024-03-14T01:53:17Z',
                      updatedAt: '2026-03-25T14:27:56Z',
                    },
                  },
                ],
              },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
    );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397257375977", title: "NIKE | SWOOSH PRO FLAT PEAK CAP (Renamed)" }) { product { id title } userErrors { field message } } }',
    });

    const titleHandleResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 5, query: "title:SWOOSH handle:nike-swoosh-pro-flat-peak-cap tag:nike product_type:ACCESSORIES") { edges { node { id title handle productType tags } } pageInfo { hasNextPage startCursor endCursor } } }',
    });

    expect(titleHandleResponse.status).toBe(200);
    expect(titleHandleResponse.body.data.products.edges).toEqual([
      {
        node: {
          id: 'gid://shopify/Product/8397257375977',
          title: 'NIKE | SWOOSH PRO FLAT PEAK CAP (Renamed)',
          handle: 'nike-swoosh-pro-flat-peak-cap',
          productType: 'ACCESSORIES',
          tags: ['cap', 'egnition-sample-data', 'nike'],
        },
      },
    ]);
    expect(titleHandleResponse.body.data.products.pageInfo).toEqual({
      hasNextPage: false,
      startCursor: 'cursor:gid://shopify/Product/8397257375977',
      endCursor: 'cursor:gid://shopify/Product/8397257375977',
    });

    const sortedResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 5, query: "tag:egnition-sample-data product_type:ACCESSORIES", sortKey: UPDATED_AT, reverse: true) { edges { node { id title handle productType tags } } pageInfo { hasNextPage startCursor endCursor } } }',
    });

    expect(sortedResponse.status).toBe(200);
    expect(sortedResponse.body.data.products.edges).toEqual([
      {
        node: {
          id: 'gid://shopify/Product/8397257375977',
          title: 'NIKE | SWOOSH PRO FLAT PEAK CAP (Renamed)',
          handle: 'nike-swoosh-pro-flat-peak-cap',
          productType: 'ACCESSORIES',
          tags: ['cap', 'egnition-sample-data', 'nike'],
        },
      },
      {
        node: {
          id: 'gid://shopify/Product/8397257081065',
          title: 'VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE',
          handle: 'vans-apparel-and-accessories-classic-super-no-show-socks-3-pack-white',
          productType: 'ACCESSORIES',
          tags: ['egnition-sample-data', 'unisex', 'vans'],
        },
      },
    ]);
    expect(sortedResponse.body.data.products.pageInfo).toEqual({
      hasNextPage: false,
      startCursor: 'cursor:gid://shopify/Product/8397257375977',
      endCursor: 'cursor:gid://shopify/Product/8397257081065',
    });
  });

  it('filters products and counts by created_at and updated_at timestamps in live-hybrid overlay reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          data: {
            productsCount: {
              count: 3,
              precision: 'EXACT',
            },
            products: {
              edges: [
                {
                  node: {
                    id: 'gid://shopify/Product/8397256720617',
                    legacyResourceId: '8397256720617',
                    title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
                    handle: 'converse-toddler-chuck-taylor-all-star-axel-mid',
                    status: 'ACTIVE',
                    vendor: 'CONVERSE',
                    productType: 'SHOES',
                    tags: ['converse', 'egnition-sample-data', 'kid'],
                    totalInventory: 45,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:02Z',
                    updatedAt: '2026-03-25T19:35:13Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8397257375977',
                    legacyResourceId: '8397257375977',
                    title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
                    handle: 'nike-swoosh-pro-flat-peak-cap',
                    status: 'ACTIVE',
                    vendor: 'NIKE',
                    productType: 'ACCESSORIES',
                    tags: ['cap', 'egnition-sample-data', 'nike'],
                    totalInventory: 20,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:33Z',
                    updatedAt: '2026-03-25T16:49:35Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8397257081065',
                    legacyResourceId: '8397257081065',
                    title: 'VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE',
                    handle: 'vans-apparel-and-accessories-classic-super-no-show-socks-3-pack-white',
                    status: 'ACTIVE',
                    vendor: 'VANS',
                    productType: 'ACCESSORIES',
                    tags: ['egnition-sample-data', 'unisex', 'vans'],
                    totalInventory: 11,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:17Z',
                    updatedAt: '2026-03-25T14:27:56Z',
                  },
                },
              ],
              pageInfo: {
                hasNextPage: false,
                endCursor: 'cursor:gid://shopify/Product/8397257081065',
              },
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query: 'query { products(first: 5) { edges { node { id title updatedAt } } } }',
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397257081065", title: "VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE (Updated)" }) { product { id title updatedAt } userErrors { field message } } }',
    });

    const createdAtResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { earlyProducts: products(first: 5, query: "created_at:<2024-03-14T01:53:20Z status:active", sortKey: CREATED_AT) { edges { node { id title createdAt } } } earlyCount: productsCount(query: "created_at:<2024-03-14T01:53:20Z status:active") { count precision } }',
    });

    expect(createdAtResponse.status).toBe(200);
    expect(createdAtResponse.body.data.earlyProducts.edges).toEqual([
      {
        node: {
          id: 'gid://shopify/Product/8397256720617',
          title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
          createdAt: '2024-03-14T01:53:02Z',
        },
      },
      {
        node: {
          id: 'gid://shopify/Product/8397257081065',
          title: 'VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE (Updated)',
          createdAt: '2024-03-14T01:53:17Z',
        },
      },
    ]);
    expect(createdAtResponse.body.data.earlyCount).toEqual({
      count: 2,
      precision: 'EXACT',
    });

    const updatedAtResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { recentProducts: products(first: 5, query: "product_type:ACCESSORIES updated_at:>=2026-03-25T14:27:57Z", sortKey: CREATED_AT) { edges { node { id title createdAt updatedAt } } } recentCount: productsCount(query: "product_type:ACCESSORIES updated_at:>=2026-03-25T14:27:57Z") { count precision } }',
    });

    expect(updatedAtResponse.status).toBe(200);
    expect(updatedAtResponse.body.data.recentProducts.edges).toHaveLength(2);
    expect(updatedAtResponse.body.data.recentProducts.edges[0].node).toEqual({
      id: 'gid://shopify/Product/8397257081065',
      title: 'VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE (Updated)',
      createdAt: '2024-03-14T01:53:17Z',
      updatedAt: '2026-03-25T14:27:57.000Z',
    });
    expect(updatedAtResponse.body.data.recentProducts.edges[1].node).toEqual({
      id: 'gid://shopify/Product/8397257375977',
      title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
      createdAt: '2024-03-14T01:53:33Z',
      updatedAt: '2026-03-25T16:49:35Z',
    });
    expect(updatedAtResponse.body.data.recentProducts.edges[0].node.updatedAt).not.toBe('2026-03-25T14:27:56Z');
    expect(updatedAtResponse.body.data.recentCount).toEqual({
      count: 2,
      precision: 'EXACT',
    });
  });

  it('supports quoted phrases, bare terms, and negated filters in products search overlay reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          data: {
            productsCount: {
              count: 3,
              precision: 'EXACT',
            },
            products: {
              edges: [
                {
                  node: {
                    id: 'gid://shopify/Product/8397256720617',
                    legacyResourceId: '8397256720617',
                    title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
                    handle: 'converse-toddler-chuck-taylor-all-star-axel-mid',
                    status: 'ACTIVE',
                    vendor: 'CONVERSE',
                    productType: 'SHOES',
                    tags: ['converse', 'egnition-sample-data', 'kid'],
                    totalInventory: 45,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:02Z',
                    updatedAt: '2026-03-25T19:35:13Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8397257375977',
                    legacyResourceId: '8397257375977',
                    title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
                    handle: 'nike-swoosh-pro-flat-peak-cap',
                    status: 'ACTIVE',
                    vendor: 'NIKE',
                    productType: 'ACCESSORIES',
                    tags: ['cap', 'egnition-sample-data', 'nike'],
                    totalInventory: 20,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:33Z',
                    updatedAt: '2026-03-25T16:49:35Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8397257081065',
                    legacyResourceId: '8397257081065',
                    title: 'VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE',
                    handle: 'vans-apparel-and-accessories-classic-super-no-show-socks-3-pack-white',
                    status: 'ACTIVE',
                    vendor: 'VANS',
                    productType: 'ACCESSORIES',
                    tags: ['egnition-sample-data', 'unisex', 'vans'],
                    totalInventory: 11,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:17Z',
                    updatedAt: '2026-03-25T14:27:56Z',
                  },
                },
              ],
              pageInfo: {
                hasNextPage: false,
                endCursor: 'cursor:gid://shopify/Product/8397257081065',
              },
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397257375977", title: "NIKE | SWOOSH PRO FLAT PEAK CAP" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query SearchGrammar($query: String!) { matches: products(first: 5, query: $query) { edges { node { id title vendor productType tags } } } count: productsCount(query: $query) { count precision } }',
        variables: {
          query: '"flat peak cap" accessories -vendor:VANS -tag:vans',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.matches.edges).toEqual([
      {
        node: {
          id: 'gid://shopify/Product/8397257375977',
          title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
          vendor: 'NIKE',
          productType: 'ACCESSORIES',
          tags: ['cap', 'egnition-sample-data', 'nike'],
        },
      },
    ]);
    expect(response.body.data.count).toEqual({
      count: 1,
      precision: 'EXACT',
    });
  });

  it('supports OR groups with field-prefix filters in products search overlay reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          data: {
            productsCount: {
              count: 3,
              precision: 'EXACT',
            },
            products: {
              edges: [
                {
                  node: {
                    id: 'gid://shopify/Product/8397256720617',
                    legacyResourceId: '8397256720617',
                    title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
                    handle: 'converse-toddler-chuck-taylor-all-star-axel-mid',
                    status: 'ACTIVE',
                    vendor: 'CONVERSE',
                    productType: 'SHOES',
                    tags: ['converse', 'egnition-sample-data', 'kid'],
                    totalInventory: 45,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:02Z',
                    updatedAt: '2026-03-25T19:35:13Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8397257375977',
                    legacyResourceId: '8397257375977',
                    title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
                    handle: 'nike-swoosh-pro-flat-peak-cap',
                    status: 'ACTIVE',
                    vendor: 'NIKE',
                    productType: 'ACCESSORIES',
                    tags: ['cap', 'egnition-sample-data', 'nike'],
                    totalInventory: 20,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:33Z',
                    updatedAt: '2026-03-25T16:49:35Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8397257081065',
                    legacyResourceId: '8397257081065',
                    title: 'VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE',
                    handle: 'vans-apparel-and-accessories-classic-super-no-show-socks-3-pack-white',
                    status: 'ACTIVE',
                    vendor: 'VANS',
                    productType: 'ACCESSORIES',
                    tags: ['egnition-sample-data', 'unisex', 'vans'],
                    totalInventory: 11,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:17Z',
                    updatedAt: '2026-03-25T14:27:56Z',
                  },
                },
              ],
              pageInfo: {
                hasNextPage: false,
                endCursor: 'cursor:gid://shopify/Product/8397257081065',
              },
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397257375977", title: "NIKE | SWOOSH PRO FLAT PEAK CAP" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query SearchOrPrefix($query: String!) { matches: products(first: 5, query: $query) { edges { node { id title vendor } } } count: productsCount(query: $query) { count precision } }',
        variables: {
          query: '(vendor:NI* OR vendor:CON*) status:active',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.matches.edges).toEqual([
      {
        node: {
          id: 'gid://shopify/Product/8397257375977',
          title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
          vendor: 'NIKE',
        },
      },
      {
        node: {
          id: 'gid://shopify/Product/8397256720617',
          title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
          vendor: 'CONVERSE',
        },
      },
    ]);
    expect(response.body.data.count).toEqual({
      count: 2,
      precision: 'EXACT',
    });
  });

  it('supports bare-text prefixes inside grouped search expressions', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          data: {
            productsCount: {
              count: 3,
              precision: 'EXACT',
            },
            products: {
              edges: [
                {
                  node: {
                    id: 'gid://shopify/Product/8397256720617',
                    legacyResourceId: '8397256720617',
                    title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
                    handle: 'converse-toddler-chuck-taylor-all-star-axel-mid',
                    status: 'ACTIVE',
                    vendor: 'CONVERSE',
                    productType: 'SHOES',
                    tags: ['converse', 'egnition-sample-data', 'kid'],
                    totalInventory: 45,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:02Z',
                    updatedAt: '2026-03-25T19:35:13Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8397257375977',
                    legacyResourceId: '8397257375977',
                    title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
                    handle: 'nike-swoosh-pro-flat-peak-cap',
                    status: 'ACTIVE',
                    vendor: 'NIKE',
                    productType: 'ACCESSORIES',
                    tags: ['cap', 'egnition-sample-data', 'nike'],
                    totalInventory: 20,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:33Z',
                    updatedAt: '2026-03-25T16:49:35Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8397257081065',
                    legacyResourceId: '8397257081065',
                    title: 'VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE',
                    handle: 'vans-apparel-and-accessories-classic-super-no-show-socks-3-pack-white',
                    status: 'ACTIVE',
                    vendor: 'VANS',
                    productType: 'ACCESSORIES',
                    tags: ['egnition-sample-data', 'unisex', 'vans'],
                    totalInventory: 11,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:17Z',
                    updatedAt: '2026-03-25T14:27:56Z',
                  },
                },
              ],
              pageInfo: {
                hasNextPage: false,
                endCursor: 'cursor:gid://shopify/Product/8397257081065',
              },
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397257375977", title: "NIKE | SWOOSH PRO FLAT PEAK CAP" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query SearchPrefixGroup($query: String!) { matches: products(first: 5, query: $query) { edges { node { id title handle } } } count: productsCount(query: $query) { count precision } }',
        variables: {
          query: '(swoo* OR handle:converse*) -tag:vans',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.matches.edges).toEqual([
      {
        node: {
          id: 'gid://shopify/Product/8397257375977',
          title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
          handle: 'nike-swoosh-pro-flat-peak-cap',
        },
      },
      {
        node: {
          id: 'gid://shopify/Product/8397256720617',
          title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
          handle: 'converse-toddler-chuck-taylor-all-star-axel-mid',
        },
      },
    ]);
    expect(response.body.data.count).toEqual({
      count: 2,
      precision: 'EXACT',
    });
  });

  it('supports NOT, tag_not, and published_at search filters across products and productsCount', async () => {
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/1',
        legacyResourceId: '1',
        title: 'Published Nike Cap',
        handle: 'published-nike-cap',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        publishedAt: '2024-02-01T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'ACCESSORIES',
        tags: ['sample', 'cap'],
        totalInventory: 1,
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
        title: 'Draft Vans Sock',
        handle: 'draft-vans-sock',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        publishedAt: null,
        vendor: 'VANS',
        productType: 'ACCESSORIES',
        tags: ['sample', 'vans'],
        totalInventory: 1,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
      {
        id: 'gid://shopify/Product/3',
        legacyResourceId: '3',
        title: 'Published Converse Shoe',
        handle: 'published-converse-shoe',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        publishedAt: '2024-03-01T00:00:00.000Z',
        vendor: 'CONVERSE',
        productType: 'SHOES',
        tags: ['sample', 'shoe'],
        totalInventory: 1,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Published Adidas Bag", vendor: "ADIDAS", productType: "ACCESSORIES", tags: ["sample", "bag"], publishedAt: "2024-04-01T00:00:00.000Z" }) { product { id title publishedAt vendor tags } userErrors { field message } } }',
    });
    const stagedProductId = createResponse.body.data.productCreate.product.id as string;

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query ProductSearchGrammar($notQuery: String!, $tagNotQuery: String!, $publishedRangeQuery: String!) {
          notMatches: products(first: 10, query: $notQuery, sortKey: PUBLISHED_AT) {
            edges { node { id vendor publishedAt } }
          }
          notCount: productsCount(query: $notQuery) { count precision }
          tagNotMatches: products(first: 10, query: $tagNotQuery, sortKey: PUBLISHED_AT) {
            edges { node { id tags publishedAt } }
          }
          tagNotCount: productsCount(query: $tagNotQuery) { count precision }
          rangeMatches: products(first: 10, query: $publishedRangeQuery, sortKey: PUBLISHED_AT) {
            edges { node { id publishedAt } }
          }
          rangeCount: productsCount(query: $publishedRangeQuery) { count precision }
        }`,
        variables: {
          notQuery: 'NOT vendor:VANS published_at:*',
          tagNotQuery: 'tag_not:vans published_at:*',
          publishedRangeQuery: "published_at:>='2024-03-01T00:00:00.000Z'",
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.notMatches.edges.map((edge: { node: { id: string } }) => edge.node.id)).toEqual([
      'gid://shopify/Product/1',
      'gid://shopify/Product/3',
      stagedProductId,
    ]);
    expect(response.body.data.notCount).toEqual({ count: 3, precision: 'EXACT' });
    expect(response.body.data.tagNotMatches.edges.map((edge: { node: { id: string } }) => edge.node.id)).toEqual([
      'gid://shopify/Product/1',
      'gid://shopify/Product/3',
      stagedProductId,
    ]);
    expect(response.body.data.tagNotCount).toEqual({ count: 3, precision: 'EXACT' });
    expect(response.body.data.rangeMatches.edges.map((edge: { node: { id: string } }) => edge.node.id)).toEqual([
      'gid://shopify/Product/3',
      stagedProductId,
    ]);
    expect(response.body.data.rangeCount).toEqual({ count: 2, precision: 'EXACT' });
  });

  it('treats later AND filters as binding tighter than ungrouped OR terms', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          data: {
            productsCount: {
              count: 3,
              precision: 'EXACT',
            },
            products: {
              edges: [
                {
                  node: {
                    id: 'gid://shopify/Product/8397256720617',
                    legacyResourceId: '8397256720617',
                    title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
                    handle: 'converse-toddler-chuck-taylor-all-star-axel-mid',
                    status: 'ACTIVE',
                    vendor: 'CONVERSE',
                    productType: 'SHOES',
                    tags: ['converse', 'egnition-sample-data', 'kid'],
                    totalInventory: 45,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:02Z',
                    updatedAt: '2026-03-25T19:35:13Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8397257375977',
                    legacyResourceId: '8397257375977',
                    title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
                    handle: 'nike-swoosh-pro-flat-peak-cap',
                    status: 'ACTIVE',
                    vendor: 'NIKE',
                    productType: 'ACCESSORIES',
                    tags: ['cap', 'egnition-sample-data', 'nike'],
                    totalInventory: 20,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:33Z',
                    updatedAt: '2026-03-25T16:49:35Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8397257081065',
                    legacyResourceId: '8397257081065',
                    title: 'VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE',
                    handle: 'vans-apparel-and-accessories-classic-super-no-show-socks-3-pack-white',
                    status: 'ACTIVE',
                    vendor: 'VANS',
                    productType: 'ACCESSORIES',
                    tags: ['egnition-sample-data', 'unisex', 'vans'],
                    totalInventory: 11,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:17Z',
                    updatedAt: '2026-03-25T14:27:56Z',
                  },
                },
              ],
              pageInfo: {
                hasNextPage: false,
                endCursor: 'cursor:gid://shopify/Product/8397257081065',
              },
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397257375977", title: "NIKE | SWOOSH PRO FLAT PEAK CAP" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query UngroupedOrPrecedence($query: String!) { matches: products(first: 10, query: $query, sortKey: UPDATED_AT, reverse: true) { edges { node { id title vendor productType tags } } } count: productsCount(query: $query) { count precision } }',
        variables: {
          query: 'vendor:CONVERSE OR tag:cap vendor:NIKE',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.matches.edges).toEqual([
      {
        node: {
          id: 'gid://shopify/Product/8397256720617',
          title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
          vendor: 'CONVERSE',
          productType: 'SHOES',
          tags: ['converse', 'egnition-sample-data', 'kid'],
        },
      },
      {
        node: {
          id: 'gid://shopify/Product/8397257375977',
          title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
          vendor: 'NIKE',
          productType: 'ACCESSORIES',
          tags: ['cap', 'egnition-sample-data', 'nike'],
        },
      },
    ]);
    expect(response.body.data.count).toEqual({
      count: 2,
      precision: 'EXACT',
    });
  });

  it('supports vendor and product-type sort keys after overlay filtering', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          data: {
            products: {
              edges: [
                {
                  node: {
                    id: 'gid://shopify/Product/8397256720617',
                    legacyResourceId: '8397256720617',
                    title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
                    handle: 'converse-toddler-chuck-taylor-all-star-axel-mid',
                    status: 'ACTIVE',
                    vendor: 'CONVERSE',
                    productType: 'SHOES',
                    tags: ['converse', 'egnition-sample-data', 'kid'],
                    totalInventory: 45,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:02Z',
                    updatedAt: '2026-03-25T19:35:13Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8397257375977',
                    legacyResourceId: '8397257375977',
                    title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
                    handle: 'nike-swoosh-pro-flat-peak-cap',
                    status: 'ACTIVE',
                    vendor: 'NIKE',
                    productType: 'ACCESSORIES',
                    tags: ['cap', 'egnition-sample-data', 'nike'],
                    totalInventory: 20,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:33Z',
                    updatedAt: '2026-03-25T16:49:35Z',
                  },
                },
                {
                  node: {
                    id: 'gid://shopify/Product/8397257081065',
                    legacyResourceId: '8397257081065',
                    title: 'VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE',
                    handle: 'vans-apparel-and-accessories-classic-super-no-show-socks-3-pack-white',
                    status: 'ACTIVE',
                    vendor: 'VANS',
                    productType: 'ACCESSORIES',
                    tags: ['egnition-sample-data', 'unisex', 'vans'],
                    totalInventory: 11,
                    tracksInventory: true,
                    createdAt: '2024-03-14T01:53:17Z',
                    updatedAt: '2026-03-25T14:27:56Z',
                  },
                },
              ],
              pageInfo: {
                hasNextPage: false,
                endCursor: 'cursor:gid://shopify/Product/8397257081065',
              },
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397257375977", title: "NIKE | SWOOSH PRO FLAT PEAK CAP" }) { product { id title } userErrors { field message } } }',
    });

    const vendorResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 5, query: "egnition-sample-data status:active", sortKey: VENDOR) { edges { node { id title vendor productType } } } }',
    });

    expect(vendorResponse.status).toBe(200);
    expect(vendorResponse.body.data.products.edges.map((edge: { node: { id: string } }) => edge.node.id)).toEqual([
      'gid://shopify/Product/8397256720617',
      'gid://shopify/Product/8397257375977',
      'gid://shopify/Product/8397257081065',
    ]);

    const productTypeResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 5, query: "egnition-sample-data status:active", sortKey: PRODUCT_TYPE, reverse: true) { edges { node { id title vendor productType } } } }',
    });

    expect(productTypeResponse.status).toBe(200);
    expect(productTypeResponse.body.data.products.edges.map((edge: { node: { id: string } }) => edge.node.id)).toEqual([
      'gid://shopify/Product/8397256720617',
      'gid://shopify/Product/8397257081065',
      'gid://shopify/Product/8397257375977',
    ]);
  });

  it('sorts snapshot products by handle, status, publishedAt, and id', async () => {
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/20',
        legacyResourceId: '20',
        title: 'Zulu Jacket',
        handle: 'zulu-jacket',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        publishedAt: '2024-03-01T00:00:00.000Z',
        vendor: 'NIKE',
        productType: 'OUTERWEAR',
        tags: ['outerwear'],
        totalInventory: 3,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
      {
        id: 'gid://shopify/Product/11',
        legacyResourceId: '11',
        title: 'Alpha Jacket',
        handle: 'alpha-jacket',
        status: 'DRAFT',
        publicationIds: [],
        createdAt: '2024-01-03T00:00:00.000Z',
        updatedAt: '2024-01-04T00:00:00.000Z',
        publishedAt: null,
        vendor: 'ADIDAS',
        productType: 'OUTERWEAR',
        tags: ['outerwear'],
        totalInventory: 5,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
      {
        id: 'gid://shopify/Product/5',
        legacyResourceId: '5',
        title: 'Middle Jacket',
        handle: 'middle-jacket',
        status: 'ARCHIVED',
        publicationIds: [],
        createdAt: '2024-01-05T00:00:00.000Z',
        updatedAt: '2024-01-06T00:00:00.000Z',
        publishedAt: '2024-02-01T00:00:00.000Z',
        vendor: 'PUMA',
        productType: 'OUTERWEAR',
        tags: ['outerwear'],
        totalInventory: 1,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const handleResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 10, sortKey: HANDLE) { edges { node { id handle status } } pageInfo { hasNextPage hasPreviousPage } } }',
    });

    expect(handleResponse.status).toBe(200);
    expect(handleResponse.body.data.products.edges.map((edge: { node: { id: string } }) => edge.node.id)).toEqual([
      'gid://shopify/Product/11',
      'gid://shopify/Product/5',
      'gid://shopify/Product/20',
    ]);

    const statusResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 10, sortKey: STATUS, reverse: true) { edges { node { id handle status } } pageInfo { hasNextPage hasPreviousPage } } }',
    });

    expect(statusResponse.status).toBe(200);
    expect(statusResponse.body.data.products.edges.map((edge: { node: { id: string } }) => edge.node.id)).toEqual([
      'gid://shopify/Product/11',
      'gid://shopify/Product/5',
      'gid://shopify/Product/20',
    ]);
    expect(
      statusResponse.body.data.products.edges.map((edge: { node: { status: string } }) => edge.node.status),
    ).toEqual(['DRAFT', 'ARCHIVED', 'ACTIVE']);

    const publishedAtResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 10, sortKey: PUBLISHED_AT, reverse: true) { edges { node { id legacyResourceId publishedAt } } pageInfo { hasNextPage hasPreviousPage } } }',
    });

    expect(publishedAtResponse.status).toBe(200);
    expect(publishedAtResponse.body.data.products.edges.map((edge: { node: { id: string } }) => edge.node.id)).toEqual([
      'gid://shopify/Product/20',
      'gid://shopify/Product/5',
      'gid://shopify/Product/11',
    ]);
    expect(
      publishedAtResponse.body.data.products.edges.map(
        (edge: { node: { publishedAt: string | null } }) => edge.node.publishedAt,
      ),
    ).toEqual(['2024-03-01T00:00:00.000Z', '2024-02-01T00:00:00.000Z', null]);

    const idResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 10, sortKey: ID, reverse: true) { edges { node { id legacyResourceId } } pageInfo { hasNextPage hasPreviousPage } } }',
    });

    expect(idResponse.status).toBe(200);
    expect(idResponse.body.data.products.edges.map((edge: { node: { id: string } }) => edge.node.id)).toEqual([
      'gid://shopify/Product/20',
      'gid://shopify/Product/11',
      'gid://shopify/Product/5',
    ]);
    expect(
      idResponse.body.data.products.edges.map(
        (edge: { node: { legacyResourceId: string } }) => edge.node.legacyResourceId,
      ),
    ).toEqual(['20', '11', '5']);
  });

  it('replays Shopify relevance ordering and opaque cursors during staged overlay reads', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementationOnce(
        async () =>
          new Response(
            JSON.stringify({
              data: {
                products: {
                  edges: [
                    {
                      cursor: 'eyJsYXN0X2lkIjo5MTAsImxhc3RfdmFsdWUiOiJzd29vc2gifQ==',
                      node: {
                        id: 'gid://shopify/Product/910',
                        legacyResourceId: '910',
                        title: 'SWOOSH Heritage Runner',
                        handle: 'swoosh-heritage-runner',
                        status: 'ACTIVE',
                        vendor: 'NIKE',
                        productType: 'SHOES',
                        tags: ['egnition-sample-data'],
                        totalInventory: 8,
                        tracksInventory: true,
                        createdAt: '2024-04-01T00:00:00.000Z',
                        updatedAt: '2024-04-09T00:00:00.000Z',
                      },
                    },
                    {
                      cursor: 'eyJsYXN0X2lkIjo5MzAsImxhc3RfdmFsdWUiOiJzd29vc2gtdGVhbSJ9',
                      node: {
                        id: 'gid://shopify/Product/930',
                        legacyResourceId: '930',
                        title: 'SWOOSH Team Sock',
                        handle: 'swoosh-team-sock',
                        status: 'ACTIVE',
                        vendor: 'NIKE',
                        productType: 'ACCESSORIES',
                        tags: ['egnition-sample-data'],
                        totalInventory: 5,
                        tracksInventory: true,
                        createdAt: '2024-04-03T00:00:00.000Z',
                        updatedAt: '2024-04-07T00:00:00.000Z',
                      },
                    },
                    {
                      cursor: 'eyJsYXN0X2lkIjo5MjAsImxhc3RfdmFsdWUiOiJzd29vc2gtY2FwIn0=',
                      node: {
                        id: 'gid://shopify/Product/920',
                        legacyResourceId: '920',
                        title: 'SWOOSH Cap',
                        handle: 'swoosh-cap',
                        status: 'ACTIVE',
                        vendor: 'NIKE',
                        productType: 'ACCESSORIES',
                        tags: ['egnition-sample-data'],
                        totalInventory: 2,
                        tracksInventory: true,
                        createdAt: '2024-04-02T00:00:00.000Z',
                        updatedAt: '2024-04-08T00:00:00.000Z',
                      },
                    },
                  ],
                  pageInfo: {
                    hasNextPage: true,
                    hasPreviousPage: false,
                    startCursor: 'eyJsYXN0X2lkIjo5MTAsImxhc3RfdmFsdWUiOiJzd29vc2gifQ==',
                    endCursor: 'eyJsYXN0X2lkIjo5MjAsImxhc3RfdmFsdWUiOiJzd29vc2gtY2FwIn0=',
                  },
                },
              },
            }),
            { status: 200, headers: { 'content-type': 'application/json' } },
          ),
      )
      .mockImplementationOnce(
        async () =>
          new Response(
            JSON.stringify({
              data: {
                products: {
                  edges: [],
                  pageInfo: {
                    hasNextPage: false,
                    hasPreviousPage: false,
                    startCursor: null,
                    endCursor: null,
                  },
                },
              },
            }),
            { status: 200, headers: { 'content-type': 'application/json' } },
          ),
      );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Unrelated Draft" }) { product { id } userErrors { field message } } }',
    });

    const requestBody = {
      query: `query ProductRelevanceReplay($query: String!) {
        products(first: 3, query: $query, sortKey: RELEVANCE) {
          edges {
            cursor
            node {
              id
              legacyResourceId
              title
              handle
            }
          }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }`,
      variables: { query: 'swoo* status:active' },
    };

    const firstResponse = await request(app).post('/admin/api/2025-01/graphql.json').send(requestBody);

    expect(firstResponse.status).toBe(200);
    expect(firstResponse.body).toEqual({
      data: {
        products: {
          edges: [
            {
              cursor: 'eyJsYXN0X2lkIjo5MTAsImxhc3RfdmFsdWUiOiJzd29vc2gifQ==',
              node: {
                id: 'gid://shopify/Product/910',
                legacyResourceId: '910',
                title: 'SWOOSH Heritage Runner',
                handle: 'swoosh-heritage-runner',
              },
            },
            {
              cursor: 'eyJsYXN0X2lkIjo5MzAsImxhc3RfdmFsdWUiOiJzd29vc2gtdGVhbSJ9',
              node: {
                id: 'gid://shopify/Product/930',
                legacyResourceId: '930',
                title: 'SWOOSH Team Sock',
                handle: 'swoosh-team-sock',
              },
            },
            {
              cursor: 'eyJsYXN0X2lkIjo5MjAsImxhc3RfdmFsdWUiOiJzd29vc2gtY2FwIn0=',
              node: {
                id: 'gid://shopify/Product/920',
                legacyResourceId: '920',
                title: 'SWOOSH Cap',
                handle: 'swoosh-cap',
              },
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: false,
            startCursor: 'eyJsYXN0X2lkIjo5MTAsImxhc3RfdmFsdWUiOiJzd29vc2gifQ==',
            endCursor: 'eyJsYXN0X2lkIjo5MjAsImxhc3RfdmFsdWUiOiJzd29vc2gtY2FwIn0=',
          },
        },
      },
    });

    const replayResponse = await request(app).post('/admin/api/2025-01/graphql.json').send(requestBody);

    expect(replayResponse.status).toBe(200);
    expect(replayResponse.body).toEqual(firstResponse.body);
    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });

  it('filters products and counts by variant sku after live-hybrid detail hydration', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
      const body =
        typeof init?.body === 'string'
          ? (JSON.parse(init.body) as { query?: string; variables?: { id?: string } })
          : {};
      const query = body.query ?? '';

      if (query.includes('product(id:')) {
        return new Response(
          JSON.stringify({
            data: {
              product: {
                id: 'gid://shopify/Product/8397256720617',
                title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
                handle: 'converse-toddler-chuck-taylor-all-star-axel-mid',
                status: 'ACTIVE',
                createdAt: '2024-03-14T01:53:02Z',
                updatedAt: '2026-03-25T19:35:13Z',
                variants: {
                  nodes: [
                    {
                      id: 'gid://shopify/ProductVariant/46789263425769',
                      title: '5 / black',
                      sku: 'C-03-black-5',
                      barcode: '1111111111111',
                    },
                  ],
                },
              },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        );
      }

      if (query.includes('productsCount')) {
        return new Response(
          JSON.stringify({
            data: {
              matches: {
                count: 1,
                precision: 'EXACT',
              },
              products: {
                edges: [
                  {
                    node: {
                      id: 'gid://shopify/Product/8397256720617',
                      title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
                      handle: 'converse-toddler-chuck-taylor-all-star-axel-mid',
                      status: 'ACTIVE',
                      createdAt: '2024-03-14T01:53:02Z',
                      updatedAt: '2026-03-25T19:35:13Z',
                    },
                  },
                ],
                pageInfo: {
                  hasNextPage: false,
                  endCursor: 'cursor:gid://shopify/Product/8397256720617',
                },
              },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        );
      }

      if (query.includes('products(')) {
        return new Response(
          JSON.stringify({
            data: {
              products: {
                edges: [
                  {
                    node: {
                      id: 'gid://shopify/Product/8397256720617',
                      title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
                      handle: 'converse-toddler-chuck-taylor-all-star-axel-mid',
                      status: 'ACTIVE',
                      createdAt: '2024-03-14T01:53:02Z',
                      updatedAt: '2026-03-25T19:35:13Z',
                    },
                  },
                ],
                pageInfo: {
                  hasNextPage: false,
                  endCursor: 'cursor:gid://shopify/Product/8397256720617',
                },
              },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        );
      }

      throw new Error(`Unexpected fetch for query: ${query}`);
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    const hydratedDetailResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id title variants(first: 10) { nodes { id title sku barcode } } } }',
        variables: { id: 'gid://shopify/Product/8397256720617' },
      });

    expect(hydratedDetailResponse.status).toBe(200);
    expect(hydratedDetailResponse.body.data.product.variants.nodes[0]).toMatchObject({
      sku: 'C-03-black-5',
      barcode: '1111111111111',
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397256720617", title: "Renamed Converse" }) { product { id title } userErrors { field message } } }',
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Unrelated Draft Cap" }) { product { id } userErrors { field message } } }',
    });

    const response = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { matches: productsCount(query: "sku:C-03-black-5") { count precision } products(first: 5, query: "sku:C-03-black-5") { edges { node { id title handle } } pageInfo { hasNextPage startCursor endCursor } } }',
    });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        matches: {
          count: 1,
          precision: 'EXACT',
        },
        products: {
          edges: [
            {
              node: {
                id: 'gid://shopify/Product/8397256720617',
                title: 'Renamed Converse',
                handle: 'converse-toddler-chuck-taylor-all-star-axel-mid',
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            startCursor: 'cursor:gid://shopify/Product/8397256720617',
            endCursor: 'cursor:gid://shopify/Product/8397256720617',
          },
        },
      },
    });
  });

  it('filters products and counts by staged variant barcode in snapshot mode', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Barcode Hat" }) { product { id } userErrors { field message } } }',
    });

    const createdId = createResponse.body.data.productCreate.product.id as string;
    const [defaultVariant] = store.getEffectiveVariantsByProductId(createdId);
    expect(defaultVariant).toBeDefined();
    const barcodeVariant = {
      ...defaultVariant!,
      sku: 'BARCODE-HAT-SKU',
      barcode: '0987654321098',
    };
    store.replaceStagedVariantsForProduct(createdId, [barcodeVariant]);

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Plain Hat" }) { product { id } userErrors { field message } } }',
    });

    const response = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { matches: productsCount(query: "barcode:0987654321098") { count precision } products(first: 5, query: "barcode:0987654321098") { edges { node { id title handle } } pageInfo { hasNextPage startCursor endCursor } } }',
    });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        matches: {
          count: 1,
          precision: 'EXACT',
        },
        products: {
          edges: [
            {
              node: {
                id: createdId,
                title: 'Barcode Hat',
                handle: 'barcode-hat',
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            startCursor: `cursor:${createdId}`,
            endCursor: `cursor:${createdId}`,
          },
        },
      },
    });
  });

  it('paginates staged products with after cursors in default overlay order', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(
      async () =>
        new Response(
          JSON.stringify({
            data: {
              products: {
                nodes: [],
              },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
    );

    const app = createApp(config).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Alpha Hat" }) { product { id } userErrors { field message } } }',
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Beta Hat" }) { product { id } userErrors { field message } } }',
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Gamma Hat" }) { product { id } userErrors { field message } } }',
    });

    const firstPage = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 2) { edges { cursor node { id title } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
    });

    expect(firstPage.status).toBe(200);
    expect(firstPage.body.data.products.edges.map((edge: { node: { title: string } }) => edge.node.title)).toEqual([
      'Gamma Hat',
      'Beta Hat',
    ]);
    expect(firstPage.body.data.products.pageInfo).toEqual({
      hasNextPage: true,
      hasPreviousPage: false,
      startCursor: firstPage.body.data.products.edges[0].cursor,
      endCursor: firstPage.body.data.products.edges[1].cursor,
    });

    const secondPage = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($after: String!) { products(first: 2, after: $after) { edges { cursor node { id title } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: {
          after: firstPage.body.data.products.pageInfo.endCursor,
        },
      });

    expect(secondPage.status).toBe(200);
    expect(secondPage.body.data.products.edges).toEqual([
      {
        cursor: expect.stringMatching(/^cursor:gid:\/\/shopify\/Product\//),
        node: {
          id: expect.stringMatching(/^gid:\/\/shopify\/Product\//),
          title: 'Alpha Hat',
        },
      },
    ]);
    expect(secondPage.body.data.products.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: true,
      startCursor: secondPage.body.data.products.edges[0].cursor,
      endCursor: secondPage.body.data.products.edges[0].cursor,
    });
  });

  it('supports before/last backward pagination in snapshot mode', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Alpha Hat" }) { product { id } userErrors { field message } } }',
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Beta Hat" }) { product { id } userErrors { field message } } }',
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Gamma Hat" }) { product { id } userErrors { field message } } }',
    });

    const anchorResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query: 'query { products(first: 3) { edges { cursor node { id title } } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($before: String!) { products(last: 2, before: $before) { edges { cursor node { id title } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: {
          before: anchorResponse.body.data.products.edges[2].cursor,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.products.edges.map((edge: { node: { title: string } }) => edge.node.title)).toEqual([
      'Gamma Hat',
      'Beta Hat',
    ]);
    expect(response.body.data.products.pageInfo).toEqual({
      hasNextPage: true,
      hasPreviousPage: false,
      startCursor: response.body.data.products.edges[0].cursor,
      endCursor: response.body.data.products.edges[1].cursor,
    });
  });

  it('applies before/last cursors after filtering and sortKey ordering during overlay reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(
      async () =>
        new Response(
          JSON.stringify({
            data: {
              products: {
                edges: [
                  {
                    node: {
                      id: 'gid://shopify/Product/8397255999721',
                      legacyResourceId: '8397255999721',
                      title: 'NIKE | CRACKLE PRINT TB TEE',
                      handle: 'nike-crackle-print-tb-tee',
                      status: 'ACTIVE',
                      vendor: 'NIKE',
                      productType: 'TOPS',
                      tags: ['nike', 'egnition-sample-data', 'tee'],
                      totalInventory: 16,
                      tracksInventory: true,
                      createdAt: '2024-03-14T01:53:01Z',
                      updatedAt: '2026-03-25T10:00:00Z',
                    },
                  },
                  {
                    node: {
                      id: 'gid://shopify/Product/8397257375977',
                      legacyResourceId: '8397257375977',
                      title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
                      handle: 'nike-swoosh-pro-flat-peak-cap',
                      status: 'ACTIVE',
                      vendor: 'NIKE',
                      productType: 'ACCESSORIES',
                      tags: ['cap', 'egnition-sample-data', 'nike'],
                      totalInventory: 20,
                      tracksInventory: true,
                      createdAt: '2024-03-14T01:53:33Z',
                      updatedAt: '2026-03-25T16:49:35Z',
                    },
                  },
                  {
                    node: {
                      id: 'gid://shopify/Product/8397257081065',
                      legacyResourceId: '8397257081065',
                      title: 'VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE',
                      handle: 'vans-apparel-and-accessories-classic-super-no-show-socks-3-pack-white',
                      status: 'ACTIVE',
                      vendor: 'VANS',
                      productType: 'ACCESSORIES',
                      tags: ['egnition-sample-data', 'unisex', 'vans'],
                      totalInventory: 11,
                      tracksInventory: true,
                      createdAt: '2024-03-14T01:53:17Z',
                      updatedAt: '2026-03-25T14:27:56Z',
                    },
                  },
                ],
              },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
    );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397257375977", title: "NIKE | SWOOSH PRO FLAT PEAK CAP (Renamed)" }) { product { id title } userErrors { field message } } }',
    });

    const latestResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 2, query: "tag:egnition-sample-data product_type:ACCESSORIES", sortKey: UPDATED_AT, reverse: true) { edges { cursor node { id title } } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($before: String!) { products(last: 1, before: $before, query: "tag:egnition-sample-data product_type:ACCESSORIES", sortKey: UPDATED_AT, reverse: true) { edges { cursor node { id title } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: {
          before: latestResponse.body.data.products.edges[1].cursor,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.products.edges).toEqual([
      {
        cursor: 'cursor:gid://shopify/Product/8397257375977',
        node: {
          id: 'gid://shopify/Product/8397257375977',
          title: 'NIKE | SWOOSH PRO FLAT PEAK CAP (Renamed)',
        },
      },
    ]);
    expect(response.body.data.products.pageInfo).toEqual({
      hasNextPage: true,
      hasPreviousPage: false,
      startCursor: 'cursor:gid://shopify/Product/8397257375977',
      endCursor: 'cursor:gid://shopify/Product/8397257375977',
    });
  });

  it('applies after cursors after filtering and sortKey ordering during overlay reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(
      async () =>
        new Response(
          JSON.stringify({
            data: {
              products: {
                edges: [
                  {
                    node: {
                      id: 'gid://shopify/Product/8397255999721',
                      legacyResourceId: '8397255999721',
                      title: 'NIKE | CRACKLE PRINT TB TEE',
                      handle: 'nike-crackle-print-tb-tee',
                      status: 'ACTIVE',
                      vendor: 'NIKE',
                      productType: 'TOPS',
                      tags: ['nike', 'egnition-sample-data', 'tee'],
                      totalInventory: 16,
                      tracksInventory: true,
                      createdAt: '2024-03-14T01:53:01Z',
                      updatedAt: '2026-03-25T10:00:00Z',
                    },
                  },
                  {
                    node: {
                      id: 'gid://shopify/Product/8397257375977',
                      legacyResourceId: '8397257375977',
                      title: 'NIKE | SWOOSH PRO FLAT PEAK CAP',
                      handle: 'nike-swoosh-pro-flat-peak-cap',
                      status: 'ACTIVE',
                      vendor: 'NIKE',
                      productType: 'ACCESSORIES',
                      tags: ['cap', 'egnition-sample-data', 'nike'],
                      totalInventory: 20,
                      tracksInventory: true,
                      createdAt: '2024-03-14T01:53:33Z',
                      updatedAt: '2026-03-25T16:49:35Z',
                    },
                  },
                  {
                    node: {
                      id: 'gid://shopify/Product/8397257081065',
                      legacyResourceId: '8397257081065',
                      title: 'VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE',
                      handle: 'vans-apparel-and-accessories-classic-super-no-show-socks-3-pack-white',
                      status: 'ACTIVE',
                      vendor: 'VANS',
                      productType: 'ACCESSORIES',
                      tags: ['egnition-sample-data', 'unisex', 'vans'],
                      totalInventory: 11,
                      tracksInventory: true,
                      createdAt: '2024-03-14T01:53:17Z',
                      updatedAt: '2026-03-25T14:27:56Z',
                    },
                  },
                ],
              },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
    );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397257375977", title: "NIKE | SWOOSH PRO FLAT PEAK CAP (Renamed)" }) { product { id title } userErrors { field message } } }',
    });

    const firstPage = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query { products(first: 1, query: "tag:egnition-sample-data product_type:ACCESSORIES", sortKey: UPDATED_AT, reverse: true) { edges { cursor node { id title } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
    });

    expect(firstPage.status).toBe(200);
    expect(firstPage.body.data.products.edges).toEqual([
      {
        cursor: 'cursor:gid://shopify/Product/8397257375977',
        node: {
          id: 'gid://shopify/Product/8397257375977',
          title: 'NIKE | SWOOSH PRO FLAT PEAK CAP (Renamed)',
        },
      },
    ]);
    expect(firstPage.body.data.products.pageInfo).toEqual({
      hasNextPage: true,
      hasPreviousPage: false,
      startCursor: 'cursor:gid://shopify/Product/8397257375977',
      endCursor: 'cursor:gid://shopify/Product/8397257375977',
    });

    const secondPage = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($after: String!) { products(first: 1, after: $after, query: "tag:egnition-sample-data product_type:ACCESSORIES", sortKey: UPDATED_AT, reverse: true) { edges { cursor node { id title } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: {
          after: 'cursor:gid://shopify/Product/8397257375977',
        },
      });

    expect(secondPage.status).toBe(200);
    expect(secondPage.body.data.products.edges).toEqual([
      {
        cursor: 'cursor:gid://shopify/Product/8397257081065',
        node: {
          id: 'gid://shopify/Product/8397257081065',
          title: 'VANS APPAREL AND ACCESSORIES | CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE',
        },
      },
    ]);
    expect(secondPage.body.data.products.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: true,
      startCursor: 'cursor:gid://shopify/Product/8397257081065',
      endCursor: 'cursor:gid://shopify/Product/8397257081065',
    });
  });

  it('preserves upstream product detail fields during overlay reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            product: {
              id: 'gid://shopify/Product/8397256720617',
              title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
              handle: 'converse-toddler-chuck-taylor-all-star-axel-mid',
              status: 'ACTIVE',
              createdAt: '2024-03-14T01:53:41Z',
              updatedAt: '2026-03-25T17:00:00Z',
              descriptionHtml:
                'The Converse Chuck Taylor All Star Axel recasts the iconic original in a refreshed silhouette for a premium look and feel.',
              onlineStorePreviewUrl:
                'https://very-big-test-store.myshopify.com/products/converse-toddler-chuck-taylor-all-star-axel-mid',
              templateSuffix: null,
              seo: {
                title: null,
                description: null,
              },
              category: {
                id: 'gid://shopify/TaxonomyCategory/aa-1',
                fullName: 'Apparel & Accessories > Shoes',
              },
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/8397256720617", title: "Renamed Converse" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id title descriptionHtml onlineStorePreviewUrl templateSuffix seo { title description } category { id fullName } } }',
        variables: { id: 'gid://shopify/Product/8397256720617' },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product).toEqual({
      id: 'gid://shopify/Product/8397256720617',
      title: 'Renamed Converse',
      descriptionHtml:
        'The Converse Chuck Taylor All Star Axel recasts the iconic original in a refreshed silhouette for a premium look and feel.',
      onlineStorePreviewUrl:
        'https://very-big-test-store.myshopify.com/products/converse-toddler-chuck-taylor-all-star-axel-mid',
      templateSuffix: null,
      seo: {
        title: null,
        description: null,
      },
      category: {
        id: 'gid://shopify/TaxonomyCategory/aa-1',
        fullName: 'Apparel & Accessories > Shoes',
      },
    });
  });

  it('returns deterministic null detail defaults for staged products without upstream hydration', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Fresh Draft" }) { product { id } userErrors { field message } } }',
    });

    const createdId = createResponse.body.data.productCreate.product.id as string;

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id title descriptionHtml onlineStorePreviewUrl templateSuffix seo { title description } category { id fullName } } }',
        variables: { id: createdId },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.product).toEqual({
      id: createdId,
      title: 'Fresh Draft',
      descriptionHtml: null,
      onlineStorePreviewUrl: null,
      templateSuffix: null,
      seo: {
        title: null,
        description: null,
      },
      category: null,
    });
  });

  it('serves top-level productVariant and inventoryItem queries from staged local variant state in snapshot mode', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Variant Detail Hat" }) { product { id title handle status } userErrors { field message } } }',
    });

    const productId = createResponse.body.data.productCreate.product.id as string;

    const productResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'query ($id: ID!) { product(id: $id) { id variants(first: 10) { nodes { id } } } }',
        variables: { id: productId },
      });

    const variantId = productResponse.body.data.product.variants.nodes[0].id as string;

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateVariant($input: ProductVariantInput!) { productVariantUpdate(input: $input) { productVariant { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping measurement { weight { unit value } } countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode } } userErrors { field message } } }',
        variables: {
          input: {
            id: variantId,
            title: 'Default Title / Red',
            sku: 'HAT-RED-1',
            barcode: 'BAR-RED-1',
            price: '29.99',
            compareAtPrice: '39.99',
            taxable: true,
            inventoryPolicy: 'DENY',
            inventoryQuantity: 7,
            selectedOptions: [{ name: 'Title', value: 'Default Title / Red' }],
            inventoryItem: {
              tracked: true,
              requiresShipping: false,
              measurement: {
                weight: {
                  unit: 'KILOGRAMS',
                  value: 1.25,
                },
              },
              countryCodeOfOrigin: 'US',
              provinceCodeOfOrigin: 'CA',
              harmonizedSystemCode: '650500',
            },
          },
        },
      });

    const inventoryItemId = updateResponse.body.data.productVariantUpdate.productVariant.inventoryItem.id as string;

    const response = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query VariantAndInventory($variantId: ID!, $inventoryItemId: ID!) { variant: productVariant(id: $variantId) { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping measurement { weight { unit value } } countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode } product { id title handle status } } stock: inventoryItem(id: $inventoryItemId) { id tracked requiresShipping measurement { weight { unit value } } countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode variant { id title sku selectedOptions { name value } product { id title handle } } } }',
      variables: { variantId, inventoryItemId },
    });

    expect(response.status).toBe(200);
    expect(response.body.data.variant).toEqual({
      id: variantId,
      title: 'Default Title / Red',
      sku: 'HAT-RED-1',
      barcode: 'BAR-RED-1',
      price: '29.99',
      compareAtPrice: '39.99',
      taxable: true,
      inventoryPolicy: 'DENY',
      inventoryQuantity: 7,
      selectedOptions: [{ name: 'Title', value: 'Default Title / Red' }],
      inventoryItem: {
        id: inventoryItemId,
        tracked: true,
        requiresShipping: false,
        measurement: {
          weight: {
            unit: 'KILOGRAMS',
            value: 1.25,
          },
        },
        countryCodeOfOrigin: 'US',
        provinceCodeOfOrigin: 'CA',
        harmonizedSystemCode: '650500',
      },
      product: {
        id: productId,
        title: 'Variant Detail Hat',
        handle: 'variant-detail-hat',
        status: 'ACTIVE',
      },
    });
    expect(response.body.data.stock).toEqual({
      id: inventoryItemId,
      tracked: true,
      requiresShipping: false,
      measurement: {
        weight: {
          unit: 'KILOGRAMS',
          value: 1.25,
        },
      },
      countryCodeOfOrigin: 'US',
      provinceCodeOfOrigin: 'CA',
      harmonizedSystemCode: '650500',
      variant: {
        id: variantId,
        title: 'Default Title / Red',
        sku: 'HAT-RED-1',
        selectedOptions: [{ name: 'Title', value: 'Default Title / Red' }],
        product: {
          id: productId,
          title: 'Variant Detail Hat',
          handle: 'variant-detail-hat',
        },
      },
    });
  });

  it('serializes inventory levels for staged top-level productVariant and inventoryItem reads', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Inventory Level Hat" }) { product { id title } userErrors { field message } } }',
    });

    const productId = createResponse.body.data.productCreate.product.id as string;

    const productResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'query ($id: ID!) { product(id: $id) { id variants(first: 10) { nodes { id } } } }',
        variables: { id: productId },
      });

    const variantId = productResponse.body.data.product.variants.nodes[0].id as string;

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateVariant($input: ProductVariantInput!) { productVariantUpdate(input: $input) { productVariant { id inventoryQuantity inventoryItem { id tracked } } userErrors { field message } } }',
        variables: {
          input: {
            id: variantId,
            inventoryQuantity: 7,
            inventoryItem: {
              tracked: true,
            },
          },
        },
      });

    const inventoryItemId = updateResponse.body.data.productVariantUpdate.productVariant.inventoryItem.id as string;

    const response = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query VariantAndInventoryLevels($variantId: ID!, $inventoryItemId: ID!) { variant: productVariant(id: $variantId) { id inventoryQuantity inventoryItem { id tracked inventoryLevels(first: 5) { edges { cursor node { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity updatedAt } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } } stock: inventoryItem(id: $inventoryItemId) { id tracked inventoryLevels(first: 5) { edges { cursor node { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity updatedAt } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } variant { id inventoryQuantity product { id title handle } } } }',
      variables: { variantId, inventoryItemId },
    });

    expect(response.status).toBe(200);
    expect(response.body.data.variant.inventoryItem.inventoryLevels).toEqual({
      edges: [
        {
          cursor: expect.stringMatching(/^cursor:/),
          node: {
            id: expect.stringMatching(/^gid:\/\/shopify\/InventoryLevel\//),
            location: {
              id: 'gid://shopify/Location/1',
              name: null,
            },
            quantities: [
              {
                name: 'available',
                quantity: 7,
                updatedAt: expect.any(String),
              },
              {
                name: 'on_hand',
                quantity: 7,
                updatedAt: null,
              },
              {
                name: 'incoming',
                quantity: 0,
                updatedAt: null,
              },
            ],
          },
        },
      ],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: expect.stringMatching(/^cursor:/),
        endCursor: expect.stringMatching(/^cursor:/),
      },
    });
    expect(response.body.data.stock).toEqual({
      id: inventoryItemId,
      tracked: true,
      inventoryLevels: {
        edges: [
          {
            cursor: expect.stringMatching(/^cursor:/),
            node: {
              id: expect.stringMatching(/^gid:\/\/shopify\/InventoryLevel\//),
              location: {
                id: 'gid://shopify/Location/1',
                name: null,
              },
              quantities: [
                {
                  name: 'available',
                  quantity: 7,
                  updatedAt: expect.any(String),
                },
                {
                  name: 'on_hand',
                  quantity: 7,
                  updatedAt: null,
                },
                {
                  name: 'incoming',
                  quantity: 0,
                  updatedAt: null,
                },
              ],
            },
          },
        ],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: expect.stringMatching(/^cursor:/),
          endCursor: expect.stringMatching(/^cursor:/),
        },
      },
      variant: {
        id: variantId,
        inventoryQuantity: 7,
        product: {
          id: productId,
          title: 'Inventory Level Hat',
          handle: 'inventory-level-hat',
        },
      },
    });
  });

  it('serializes selection-aware inventory fields on product-scoped variant connection reads', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Scoped Inventory Hat" }) { product { id title } userErrors { field message } } }',
    });

    const productId = createResponse.body.data.productCreate.product.id as string;

    const productResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'query ($id: ID!) { product(id: $id) { id variants(first: 10) { nodes { id } } } }',
        variables: { id: productId },
      });

    const variantId = productResponse.body.data.product.variants.nodes[0].id as string;

    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateVariant($input: ProductVariantInput!) { productVariantUpdate(input: $input) { productVariant { id inventoryQuantity inventoryItem { id tracked requiresShipping } } userErrors { field message } } }',
        variables: {
          input: {
            id: variantId,
            inventoryQuantity: 4,
            inventoryItem: {
              tracked: false,
              requiresShipping: true,
            },
          },
        },
      });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id variants(first: 10) { edges { cursor node { id inventoryQuantity inventoryItem { id tracked inventoryLevels(first: 5) { nodes { id quantities(names: ["available"]) { name quantity updatedAt } } } } } } } } }',
        variables: { id: productId },
      });

    expect(response.status).toBe(200);
    const node = response.body.data.product.variants.edges[0].node;
    expect(node).toEqual({
      id: variantId,
      inventoryQuantity: 4,
      inventoryItem: {
        id: expect.stringMatching(/^gid:\/\/shopify\/InventoryItem\//),
        tracked: false,
        inventoryLevels: {
          nodes: [
            {
              id: expect.stringMatching(/^gid:\/\/shopify\/InventoryLevel\//),
              quantities: [
                {
                  name: 'available',
                  quantity: 4,
                  updatedAt: expect.any(String),
                },
              ],
            },
          ],
        },
      },
    });
    expect(Object.keys(node.inventoryItem)).toEqual(['id', 'tracked', 'inventoryLevels']);
  });

  it('serves top-level inventoryLevel reads from product-backed inventory items in snapshot mode', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Inventory Level Root Hat" }) { product { id title } userErrors { field message } } }',
    });

    const productId = createResponse.body.data.productCreate.product.id as string;

    const productResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'query ($id: ID!) { product(id: $id) { id variants(first: 10) { nodes { id } } } }',
        variables: { id: productId },
      });

    const variantId = productResponse.body.data.product.variants.nodes[0].id as string;

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateVariant($input: ProductVariantInput!) { productVariantUpdate(input: $input) { productVariant { id inventoryQuantity inventoryItem { id tracked } } userErrors { field message } } }',
        variables: {
          input: {
            id: variantId,
            inventoryQuantity: 9,
            inventoryItem: {
              tracked: true,
            },
          },
        },
      });

    const inventoryItemId = updateResponse.body.data.productVariantUpdate.productVariant.inventoryItem.id as string;

    const connectionResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'query ($inventoryItemId: ID!) { inventoryItem(id: $inventoryItemId) { id inventoryLevels(first: 5) { nodes { id } } } }',
      variables: { inventoryItemId },
    });

    const inventoryLevelId = connectionResponse.body.data.inventoryItem.inventoryLevels.nodes[0].id as string;

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($inventoryLevelId: ID!, $missingInventoryLevelId: ID!) { level: inventoryLevel(id: $inventoryLevelId) { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity updatedAt } item { id sku tracked variant { id inventoryQuantity product { id title handle } } } } missing: inventoryLevel(id: $missingInventoryLevelId) { id } }',
        variables: {
          inventoryLevelId,
          missingInventoryLevelId: 'gid://shopify/InventoryLevel/999999?inventory_item_id=999999',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      level: {
        id: inventoryLevelId,
        location: {
          id: 'gid://shopify/Location/1',
          name: null,
        },
        quantities: [
          {
            name: 'available',
            quantity: 9,
            updatedAt: expect.any(String),
          },
          {
            name: 'on_hand',
            quantity: 9,
            updatedAt: null,
          },
          {
            name: 'incoming',
            quantity: 0,
            updatedAt: null,
          },
        ],
        item: {
          id: inventoryItemId,
          sku: null,
          tracked: true,
          variant: {
            id: variantId,
            inventoryQuantity: 9,
            product: {
              id: productId,
              title: 'Inventory Level Root Hat',
              handle: 'inventory-level-root-hat',
            },
          },
        },
      },
      missing: null,
    });
  });

  it('replays hydrated inventory levels on top-level productVariant and inventoryItem reads in live-hybrid mode', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (_input, init) => {
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as { query?: string }) : {};
      const query = body.query ?? '';

      if (query.includes('product(id:')) {
        return new Response(
          JSON.stringify({
            data: {
              product: {
                id: 'gid://shopify/Product/700',
                title: 'Hydrated Inventory Level Hat',
                handle: 'hydrated-inventory-level-hat',
                status: 'ACTIVE',
                createdAt: '2024-01-02T00:00:00.000Z',
                updatedAt: '2024-01-03T00:00:00.000Z',
                variants: {
                  nodes: [
                    {
                      id: 'gid://shopify/ProductVariant/1700',
                      title: 'Default Title',
                      sku: 'INV-LVL-1',
                      barcode: null,
                      price: '24.00',
                      compareAtPrice: null,
                      taxable: true,
                      inventoryPolicy: 'DENY',
                      inventoryQuantity: 5,
                      selectedOptions: [{ name: 'Title', value: 'Default Title' }],
                      inventoryItem: {
                        id: 'gid://shopify/InventoryItem/1701',
                        tracked: true,
                        requiresShipping: true,
                        measurement: {
                          weight: {
                            unit: 'KILOGRAMS',
                            value: 0.5,
                          },
                        },
                        countryCodeOfOrigin: 'US',
                        provinceCodeOfOrigin: 'CA',
                        harmonizedSystemCode: '650500',
                        inventoryLevels: {
                          edges: [
                            {
                              cursor: 'opaque-live-level-cursor',
                              node: {
                                id: 'gid://shopify/InventoryLevel/1701?inventory_item_id=1701',
                                location: {
                                  id: 'gid://shopify/Location/68509171945',
                                  name: '103 ossington',
                                },
                                quantities: [
                                  { name: 'available', quantity: 5, updatedAt: '2026-04-17T03:58:00Z' },
                                  { name: 'on_hand', quantity: 5, updatedAt: null },
                                  { name: 'incoming', quantity: 0, updatedAt: null },
                                ],
                              },
                            },
                          ],
                        },
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

      return new Response(JSON.stringify({ data: {} }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query Hydrate($id: ID!) { product(id: $id) { id title handle status variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping measurement { weight { unit value } } countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode inventoryLevels(first: 5) { edges { cursor node { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity updatedAt } } } } } } } } }',
        variables: { id: 'gid://shopify/Product/700' },
      });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/700", title: "Hydrated Inventory Level Hat Renamed" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query VariantAndInventoryLevels($variantId: ID!, $inventoryItemId: ID!) { variant: productVariant(id: $variantId) { id title inventoryQuantity inventoryItem { id tracked inventoryLevels(first: 5) { edges { cursor node { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity updatedAt } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } product { id title handle status } } stock: inventoryItem(id: $inventoryItemId) { id tracked inventoryLevels(first: 5) { edges { cursor node { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity updatedAt } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } variant { id inventoryQuantity product { id title handle status } } } }',
        variables: {
          variantId: 'gid://shopify/ProductVariant/1700',
          inventoryItemId: 'gid://shopify/InventoryItem/1701',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.variant).toEqual({
      id: 'gid://shopify/ProductVariant/1700',
      title: 'Default Title',
      inventoryQuantity: 5,
      inventoryItem: {
        id: 'gid://shopify/InventoryItem/1701',
        tracked: true,
        inventoryLevels: {
          edges: [
            {
              cursor: 'opaque-live-level-cursor',
              node: {
                id: 'gid://shopify/InventoryLevel/1701?inventory_item_id=1701',
                location: {
                  id: 'gid://shopify/Location/68509171945',
                  name: '103 ossington',
                },
                quantities: [
                  { name: 'available', quantity: 5, updatedAt: '2026-04-17T03:58:00Z' },
                  { name: 'on_hand', quantity: 5, updatedAt: null },
                  { name: 'incoming', quantity: 0, updatedAt: null },
                ],
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'opaque-live-level-cursor',
            endCursor: 'opaque-live-level-cursor',
          },
        },
      },
      product: {
        id: 'gid://shopify/Product/700',
        title: 'Hydrated Inventory Level Hat Renamed',
        handle: 'hydrated-inventory-level-hat',
        status: 'ACTIVE',
      },
    });
    expect(response.body.data.stock).toEqual({
      id: 'gid://shopify/InventoryItem/1701',
      tracked: true,
      inventoryLevels: {
        edges: [
          {
            cursor: 'opaque-live-level-cursor',
            node: {
              id: 'gid://shopify/InventoryLevel/1701?inventory_item_id=1701',
              location: {
                id: 'gid://shopify/Location/68509171945',
                name: '103 ossington',
              },
              quantities: [
                { name: 'available', quantity: 5, updatedAt: '2026-04-17T03:58:00Z' },
                { name: 'on_hand', quantity: 5, updatedAt: null },
                { name: 'incoming', quantity: 0, updatedAt: null },
              ],
            },
          },
        ],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: 'opaque-live-level-cursor',
          endCursor: 'opaque-live-level-cursor',
        },
      },
      variant: {
        id: 'gid://shopify/ProductVariant/1700',
        inventoryQuantity: 5,
        product: {
          id: 'gid://shopify/Product/700',
          title: 'Hydrated Inventory Level Hat Renamed',
          handle: 'hydrated-inventory-level-hat',
          status: 'ACTIVE',
        },
      },
    });
  });

  it('overlays top-level productVariant and inventoryItem queries onto hydrated variant state in live-hybrid mode', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (_input, init) => {
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as { query?: string }) : {};
      const query = body.query ?? '';

      if (query.includes('product(id:')) {
        return new Response(
          JSON.stringify({
            data: {
              product: {
                id: 'gid://shopify/Product/500',
                title: 'Hydrated Variant Hat',
                handle: 'hydrated-variant-hat',
                status: 'ACTIVE',
                createdAt: '2024-01-02T00:00:00.000Z',
                updatedAt: '2024-01-03T00:00:00.000Z',
                variants: {
                  nodes: [
                    {
                      id: 'gid://shopify/ProductVariant/900',
                      title: 'Small / Black',
                      sku: 'BASE-SM-BLK',
                      barcode: 'BASE-BAR',
                      price: '19.99',
                      compareAtPrice: '24.99',
                      taxable: true,
                      inventoryPolicy: 'DENY',
                      inventoryQuantity: 11,
                      selectedOptions: [
                        { name: 'Size', value: 'Small' },
                        { name: 'Color', value: 'Black' },
                      ],
                      inventoryItem: {
                        id: 'gid://shopify/InventoryItem/901',
                        tracked: true,
                        requiresShipping: true,
                        measurement: {
                          weight: {
                            unit: 'KILOGRAMS',
                            value: 0.5,
                          },
                        },
                        countryCodeOfOrigin: 'US',
                        provinceCodeOfOrigin: 'CA',
                        harmonizedSystemCode: '650500',
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

      return new Response(JSON.stringify({ data: {} }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query Hydrate($id: ID!) { product(id: $id) { id title handle status variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping measurement { weight { unit value } } countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode } } } } }',
        variables: { id: 'gid://shopify/Product/500' },
      });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/500", title: "Hydrated Variant Hat Renamed" }) { product { id title } userErrors { field message } } }',
    });

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query VariantAndInventory($variantId: ID!, $inventoryItemId: ID!) { variant: productVariant(id: $variantId) { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping measurement { weight { unit value } } countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode } product { id title handle status } } stock: inventoryItem(id: $inventoryItemId) { id tracked requiresShipping measurement { weight { unit value } } countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode variant { id title sku product { id title handle status } } } }',
        variables: {
          variantId: 'gid://shopify/ProductVariant/900',
          inventoryItemId: 'gid://shopify/InventoryItem/901',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.variant).toEqual({
      id: 'gid://shopify/ProductVariant/900',
      title: 'Small / Black',
      sku: 'BASE-SM-BLK',
      barcode: 'BASE-BAR',
      price: '19.99',
      compareAtPrice: '24.99',
      taxable: true,
      inventoryPolicy: 'DENY',
      inventoryQuantity: 11,
      selectedOptions: [
        { name: 'Size', value: 'Small' },
        { name: 'Color', value: 'Black' },
      ],
      inventoryItem: {
        id: 'gid://shopify/InventoryItem/901',
        tracked: true,
        requiresShipping: true,
        measurement: {
          weight: {
            unit: 'KILOGRAMS',
            value: 0.5,
          },
        },
        countryCodeOfOrigin: 'US',
        provinceCodeOfOrigin: 'CA',
        harmonizedSystemCode: '650500',
      },
      product: {
        id: 'gid://shopify/Product/500',
        title: 'Hydrated Variant Hat Renamed',
        handle: 'hydrated-variant-hat',
        status: 'ACTIVE',
      },
    });
    expect(response.body.data.stock).toEqual({
      id: 'gid://shopify/InventoryItem/901',
      tracked: true,
      requiresShipping: true,
      measurement: {
        weight: {
          unit: 'KILOGRAMS',
          value: 0.5,
        },
      },
      countryCodeOfOrigin: 'US',
      provinceCodeOfOrigin: 'CA',
      harmonizedSystemCode: '650500',
      variant: {
        id: 'gid://shopify/ProductVariant/900',
        title: 'Small / Black',
        sku: 'BASE-SM-BLK',
        product: {
          id: 'gid://shopify/Product/500',
          title: 'Hydrated Variant Hat Renamed',
          handle: 'hydrated-variant-hat',
          status: 'ACTIVE',
        },
      },
    });
  });
});
