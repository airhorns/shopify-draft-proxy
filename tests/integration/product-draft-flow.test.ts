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
    delete process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'];
    delete process.env['SHOPIFY_CONFORMANCE_APP_API_KEY'];
    delete process.env['SHOPIFY_CONFORMANCE_APP_ID'];
  });

  it('stages productCreate locally and returns it from a subsequent product query without upstream mutation', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async (input) => {
      const url = typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url;
      if (url.endsWith('/graphql.json')) {
        return new Response(JSON.stringify({ data: { product: null } }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }

      throw new Error(`Unexpected fetch: ${String(url)}`);
    });

    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query:
          'mutation productCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle status createdAt updatedAt } userErrors { field message } } }',
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

    const initialQueryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id title totalInventory tracksInventory options { id name position optionValues { id name hasVariants } } variants(first: 10) { nodes { id title inventoryQuantity inventoryItem { id tracked requiresShipping } } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: { id: createdId },
      });

    expect(initialQueryResponse.status).toBe(200);
    expect(initialQueryResponse.body.data.product.totalInventory).toBe(0);
    expect(initialQueryResponse.body.data.product.tracksInventory).toBe(false);
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
      inventoryQuantity: 0,
      inventoryItem: {
        id: expect.stringMatching(/^gid:\/\/shopify\/InventoryItem\//),
        tracked: false,
        requiresShipping: true,
      },
    });
    expect(initialQueryResponse.body.data.product.variants.pageInfo).toMatchObject({
      hasNextPage: false,
      hasPreviousPage: false,
    });

    const defaultVariant = initialQueryResponse.body.data.product.variants.nodes[0];
    const stockResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($inventoryItemId: ID!) { inventoryItem(id: $inventoryItemId) { id tracked requiresShipping variant { id inventoryQuantity product { id totalInventory tracksInventory } } } }',
        variables: { inventoryItemId: defaultVariant.inventoryItem.id },
      });

    expect(stockResponse.status).toBe(200);
    expect(stockResponse.body.data.inventoryItem).toEqual({
      id: defaultVariant.inventoryItem.id,
      tracked: false,
      requiresShipping: true,
      variant: {
        id: defaultVariant.id,
        inventoryQuantity: 0,
        product: {
          id: createdId,
          totalInventory: 0,
          tracksInventory: false,
        },
      },
    });

    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation ($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title } userErrors { field message } } }',
        variables: {
          product: {
            id: createdId,
            title: 'Variant Hat Renamed',
          },
        },
      });

    const updatedQueryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id title options { id name position optionValues { id name hasVariants } } variants(first: 10) { nodes { id title inventoryQuantity inventoryItem { id tracked requiresShipping } } } } }',
        variables: { id: createdId },
      });

    expect(updatedQueryResponse.status).toBe(200);
    expect(updatedQueryResponse.body.data.product.title).toBe('Variant Hat Renamed');
    expect(updatedQueryResponse.body.data.product.options).toEqual(initialQueryResponse.body.data.product.options);
    expect(updatedQueryResponse.body.data.product.variants.nodes).toEqual(
      initialQueryResponse.body.data.product.variants.nodes,
    );
    expect(fetchSpy).toHaveBeenCalledTimes(3);
  });

  it('stages metafields for product variant owners and exposes them through product and productVariant reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Variant Metafield Hat" }) { product { id variants(first: 1) { nodes { id metafield(namespace: "custom", key: "care") { id } metafields(first: 10) { nodes { id } pageInfo { hasNextPage hasPreviousPage } } } } } userErrors { field message } } }',
    });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.productCreate.userErrors).toEqual([]);
    const productId = createResponse.body.data.productCreate.product.id as string;
    const variant = createResponse.body.data.productCreate.product.variants.nodes[0] as {
      id: string;
      metafield: unknown;
      metafields: { nodes: unknown[]; pageInfo: { hasNextPage: boolean; hasPreviousPage: boolean } };
    };
    expect(variant.metafield).toBeNull();
    expect(variant.metafields).toEqual({
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
          'mutation SetVariantMetafield($metafields: [MetafieldsSetInput!]!) { metafieldsSet(metafields: $metafields) { metafields { id namespace key type value ownerType compareDigest } userErrors { field message code elementIndex } } }',
        variables: {
          metafields: [
            {
              ownerId: variant.id,
              namespace: 'custom',
              key: 'care',
              type: 'single_line_text_field',
              value: 'Spot clean',
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
        key: 'care',
        type: 'single_line_text_field',
        value: 'Spot clean',
        ownerType: 'PRODUCTVARIANT',
        compareDigest: expect.stringMatching(/^draft:/),
      },
    ]);

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query VariantMetafields($productId: ID!, $variantId: ID!) {
          product(id: $productId) {
            id
            variants(first: 1) {
              nodes {
                id
                care: metafield(namespace: "custom", key: "care") { id namespace key value ownerType }
                metafields(first: 10) {
                  nodes { id namespace key value ownerType }
                  pageInfo { hasNextPage hasPreviousPage }
                }
              }
            }
          }
          productVariant(id: $variantId) {
            id
            care: metafield(namespace: "custom", key: "care") { id namespace key value ownerType }
            metafields(first: 10) {
              nodes { id namespace key value ownerType }
              pageInfo { hasNextPage hasPreviousPage }
            }
          }
        }`,
        variables: {
          productId,
          variantId: variant.id,
        },
      });

    expect(readResponse.status).toBe(200);
    const productVariantNode = readResponse.body.data.product.variants.nodes[0];
    expect(productVariantNode.care).toMatchObject({
      namespace: 'custom',
      key: 'care',
      value: 'Spot clean',
      ownerType: 'PRODUCTVARIANT',
    });
    expect(productVariantNode.metafields.nodes).toEqual([productVariantNode.care]);
    expect(productVariantNode.metafields.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
    });
    expect(readResponse.body.data.productVariant).toEqual(productVariantNode);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('rejects blank productCreate titles with Shopify-like userErrors', async () => {
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateBlankProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }',
        variables: {
          product: {
            title: '',
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.productCreate).toEqual({
      product: null,
      userErrors: [{ field: ['title'], message: "Title can't be blank" }],
    });
  });

  it('keeps the existing product payload and returns a Shopify-like userError when productUpdate title is blank', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Blank Update Guard Product',
          },
        },
      });

    const createdProduct = createResponse.body.data.productCreate.product;

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateBlankProduct($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            id: createdProduct.id,
            title: '',
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.productUpdate).toEqual({
      product: createdProduct,
      userErrors: [{ field: ['title'], message: "Title can't be blank" }],
    });
  });

  it('rejects unknown productUpdate ids in snapshot mode instead of staging phantom products', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateUnknownProduct($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title } userErrors { field message } } }',
        variables: {
          product: {
            id: 'gid://shopify/Product/999999999999999',
            title: 'Ghost Product',
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.productUpdate).toEqual({
      product: null,
      userErrors: [{ field: ['id'], message: 'Product does not exist' }],
    });
  });

  it('rejects unknown productDelete ids in snapshot mode instead of reporting a fake deletion', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeleteUnknownProduct($input: ProductDeleteInput!) { productDelete(input: $input) { deletedProductId userErrors { field message } } }',
        variables: {
          input: {
            id: 'gid://shopify/Product/999999999999999',
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.productDelete).toEqual({
      deletedProductId: null,
      userErrors: [{ field: ['id'], message: 'Product does not exist' }],
    });
  });

  it('mirrors Shopify userErrors when productUpdate omits id entirely', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateWithoutId($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Missing Id Product',
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.productUpdate).toEqual({
      product: null,
      userErrors: [{ field: ['id'], message: 'Product does not exist' }],
    });
  });

  it('returns Shopify-like INVALID_VARIABLE errors when productDelete variables omit input.id', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeleteWithoutId($input: ProductDeleteInput!) { productDelete(input: $input) { deletedProductId userErrors { field message } } }',
        variables: {
          input: {},
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.errors).toEqual([
      {
        message:
          'Variable $input of type ProductDeleteInput! was provided invalid value for id (Expected value to not be null)',
        locations: [{ line: expect.any(Number), column: expect.any(Number) }],
        extensions: {
          code: 'INVALID_VARIABLE',
          value: {},
          problems: [{ path: ['id'], explanation: 'Expected value to not be null' }],
        },
      },
    ]);
  });

  it('returns Shopify-like argument validation errors when productDelete inlines a missing required input id', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const response = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query: 'mutation { productDelete(input: {}) { deletedProductId userErrors { field message } } }',
    });

    expect(response.status).toBe(200);
    expect(response.body.errors).toEqual([
      {
        message: "Argument 'id' on InputObject 'ProductDeleteInput' is required. Expected type ID!",
        locations: [{ line: expect.any(Number), column: expect.any(Number) }],
        path: ['mutation', 'productDelete', 'input', 'id'],
        extensions: {
          code: 'missingRequiredInputObjectAttribute',
          argumentName: 'id',
          argumentType: 'ID!',
          inputObjectType: 'ProductDeleteInput',
        },
      },
    ]);
  });

  it('returns Shopify-like argument validation errors when productDelete inlines a null input id', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const response = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query: 'mutation { productDelete(input: { id: null }) { deletedProductId userErrors { field message } } }',
    });

    expect(response.status).toBe(200);
    expect(response.body.errors).toEqual([
      {
        message: "Argument 'id' on InputObject 'ProductDeleteInput' has an invalid value (null). Expected type 'ID!'.",
        locations: [{ line: expect.any(Number), column: expect.any(Number) }],
        path: ['mutation', 'productDelete', 'input', 'id'],
        extensions: {
          code: 'argumentLiteralsIncompatible',
          argumentName: 'id',
          typeName: 'InputObject',
        },
      },
    ]);
  });

  it('auto-generates unique staged handles when productCreate titles collide', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const firstResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Collision Hat',
          },
        },
      });

    const secondResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Collision Hat',
          },
        },
      });

    expect(firstResponse.status).toBe(200);
    expect(secondResponse.status).toBe(200);
    expect(firstResponse.body.data.productCreate.userErrors).toEqual([]);
    expect(secondResponse.body.data.productCreate.userErrors).toEqual([]);
    expect(firstResponse.body.data.productCreate.product.handle).toBe('collision-hat');
    expect(secondResponse.body.data.productCreate.product.handle).toBe('collision-hat-1');
  });

  it('normalizes explicit productCreate handles before storing them', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Normalized Handle Owner',
            handle: '  Weird Handle / 100%  ',
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.productCreate).toEqual({
      product: {
        id: expect.any(String),
        title: 'Normalized Handle Owner',
        handle: 'weird-handle-100',
      },
      userErrors: [],
    });
  });

  it("normalizes punctuation-only explicit productCreate handles to Shopify's product fallback slug", async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Punctuation Handle Owner',
            handle: '%%%',
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.productCreate).toEqual({
      product: {
        id: expect.any(String),
        title: 'Punctuation Handle Owner',
        handle: 'product',
      },
      userErrors: [],
    });
  });

  it('deduplicates punctuation-only explicit productCreate handles when the product fallback slug is already taken', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const firstCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'First Punctuation Handle Owner',
            handle: '%%%',
          },
        },
      });

    const secondCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Second Punctuation Handle Owner',
            handle: '%%%',
          },
        },
      });

    expect(firstCreateResponse.status).toBe(200);
    expect(secondCreateResponse.status).toBe(200);
    expect(firstCreateResponse.body.data.productCreate).toEqual({
      product: {
        id: expect.any(String),
        title: 'First Punctuation Handle Owner',
        handle: 'product',
      },
      userErrors: [],
    });
    expect(secondCreateResponse.body.data.productCreate).toEqual({
      product: {
        id: expect.any(String),
        title: 'Second Punctuation Handle Owner',
        handle: 'product-1',
      },
      userErrors: [],
    });
  });

  it('rejects explicit colliding handles for productCreate with Shopify-like userErrors', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const firstResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Handle Owner',
            handle: 'handle-owner',
          },
        },
      });

    expect(firstResponse.status).toBe(200);
    expect(firstResponse.body.data.productCreate.userErrors).toEqual([]);

    const collisionResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Handle Collision',
            handle: 'handle-owner',
          },
        },
      });

    expect(collisionResponse.status).toBe(200);
    expect(collisionResponse.body.data.productCreate).toEqual({
      product: null,
      userErrors: [
        { field: ['input', 'handle'], message: "Handle 'handle-owner' already in use. Please provide a new handle." },
      ],
    });
  });

  it('normalizes explicit productUpdate handles before storing them', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Update Handle Owner',
          },
        },
      });

    const productId = createResponse.body.data.productCreate.product.id as string;

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateProduct($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            id: productId,
            handle: '  Mixed CASE/ Weird 200 % ',
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.productUpdate).toEqual({
      product: {
        id: productId,
        title: 'Update Handle Owner',
        handle: 'mixed-case-weird-200',
      },
      userErrors: [],
    });
  });

  it("normalizes punctuation-only explicit productUpdate handles to Shopify's product fallback slug", async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Update Punctuation Handle Owner',
          },
        },
      });

    const productId = createResponse.body.data.productCreate.product.id as string;

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateProduct($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            id: productId,
            handle: '%%%',
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.productUpdate).toEqual({
      product: {
        id: productId,
        title: 'Update Punctuation Handle Owner',
        handle: 'product',
      },
      userErrors: [],
    });
  });

  it('deduplicates punctuation-only explicit productUpdate handles when the product fallback slug is already taken', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const firstCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Fallback Handle Owner',
            handle: '%%%',
          },
        },
      });

    const secondCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Fallback Handle Challenger',
          },
        },
      });

    const secondProductId = secondCreateResponse.body.data.productCreate.product.id as string;

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateProduct($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            id: secondProductId,
            handle: '%%%',
          },
        },
      });

    expect(firstCreateResponse.status).toBe(200);
    expect(secondCreateResponse.status).toBe(200);
    expect(updateResponse.status).toBe(200);
    expect(firstCreateResponse.body.data.productCreate.product.handle).toBe('product');
    expect(updateResponse.body.data.productUpdate).toEqual({
      product: {
        id: secondProductId,
        title: 'Fallback Handle Challenger',
        handle: 'product-1',
      },
      userErrors: [],
    });
  });

  it('preserves the current handle when productUpdate tries to claim a different products handle', async () => {
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const firstCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'First Handle Owner',
          },
        },
      });
    const secondCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Second Handle Owner',
          },
        },
      });

    const firstProductId = firstCreateResponse.body.data.productCreate.product.id as string;
    const firstHandle = firstCreateResponse.body.data.productCreate.product.handle as string;
    const secondProductId = secondCreateResponse.body.data.productCreate.product.id as string;
    const secondHandle = secondCreateResponse.body.data.productCreate.product.handle as string;

    const updateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateProduct($product: ProductUpdateInput!) { productUpdate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            id: secondProductId,
            handle: firstHandle,
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.productUpdate).toEqual({
      product: {
        id: secondProductId,
        title: 'Second Handle Owner',
        handle: secondHandle,
      },
      userErrors: [
        { field: ['input', 'handle'], message: `Handle '${firstHandle}' already in use. Please provide a new handle.` },
      ],
    });

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'query ReadProduct($id: ID!) { product(id: $id) { id title handle } }',
        variables: { id: secondProductId },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body.data.product).toEqual({
      id: secondProductId,
      title: 'Second Handle Owner',
      handle: secondHandle,
    });
    expect(firstProductId).not.toBe(secondProductId);
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
      tags: ['existing', 'sale', 'summer'],
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
      tags: ['existing', 'sale', 'summer'],
    });
    expect(queryResponse.body.data.products.nodes).toEqual([
      {
        id: productId,
        tags: ['existing', 'sale', 'summer'],
      },
    ]);
    expect(queryResponse.body.data.productsCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });
  });

  it('keeps newly added hydrated product tags out of immediate tag-filtered search results to mirror Shopify indexing lag', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
      const body = JSON.parse(String(init?.body ?? '{}')) as { query?: string };
      const query = body.query ?? '';

      if (query.includes('products(first: 10, query: "tag:hermes-sale-live")')) {
        return new Response(
          JSON.stringify({
            data: {
              product: {
                id: 'gid://shopify/Product/9255305347305',
                title: 'Hydrated Tagged Product',
                handle: 'hydrated-tagged-product',
                status: 'ACTIVE',
                createdAt: '2024-01-01T00:00:00.000Z',
                updatedAt: '2024-01-02T00:00:00.000Z',
                vendor: 'NIKE',
                productType: 'ACCESSORIES',
                tags: ['existing'],
                totalInventory: 4,
                tracksInventory: true,
              },
              products: { nodes: [] },
              productsCount: { count: 0, precision: 'EXACT' },
            },
          }),
          {
            status: 200,
            headers: { 'content-type': 'application/json' },
          },
        );
      }

      if (query.includes('products(first: 10, query: "tag:hermes-summer-live")')) {
        return new Response(
          JSON.stringify({
            data: {
              product: {
                id: 'gid://shopify/Product/9255305347305',
                title: 'Hydrated Tagged Product',
                handle: 'hydrated-tagged-product',
                status: 'ACTIVE',
                createdAt: '2024-01-01T00:00:00.000Z',
                updatedAt: '2024-01-02T00:00:00.000Z',
                vendor: 'NIKE',
                productType: 'ACCESSORIES',
                tags: ['existing'],
                totalInventory: 4,
                tracksInventory: true,
              },
              remaining: { nodes: [] },
              removed: { nodes: [] },
              remainingCount: { count: 0, precision: 'EXACT' },
              removedCount: { count: 0, precision: 'EXACT' },
            },
          }),
          {
            status: 200,
            headers: { 'content-type': 'application/json' },
          },
        );
      }

      if (query.includes('product(id: $id)')) {
        return new Response(
          JSON.stringify({
            data: {
              product: {
                id: 'gid://shopify/Product/9255305347305',
                title: 'Hydrated Tagged Product',
                handle: 'hydrated-tagged-product',
                status: 'ACTIVE',
                createdAt: '2024-01-01T00:00:00.000Z',
                updatedAt: '2024-01-02T00:00:00.000Z',
                vendor: 'NIKE',
                productType: 'ACCESSORIES',
                tags: ['existing'],
                totalInventory: 4,
                tracksInventory: true,
              },
            },
          }),
          {
            status: 200,
            headers: { 'content-type': 'application/json' },
          },
        );
      }

      throw new Error(`Unexpected fetch during hydrated tagsAdd indexing-lag test: ${String(input)}`);
    });

    const app = createApp(config).callback();
    const productId = 'gid://shopify/Product/9255305347305';

    const hydrateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query HydrateTaggedProduct($id: ID!) { product(id: $id) { id title handle status vendor productType tags totalInventory tracksInventory createdAt updatedAt } }',
        variables: { id: productId },
      });

    expect(hydrateResponse.status).toBe(200);
    expect(hydrateResponse.body.data.product.tags).toEqual(['existing']);

    const addTagsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation AddTags($id: ID!, $tags: [String!]!) { tagsAdd(id: $id, tags: $tags) { node { ... on Product { id tags } } userErrors { field message } } }',
        variables: {
          id: productId,
          tags: ['existing', 'hermes-summer-live', 'hermes-sale-live'],
        },
      });

    expect(addTagsResponse.status).toBe(200);
    expect(addTagsResponse.body.data.tagsAdd.userErrors).toEqual([]);
    expect(addTagsResponse.body.data.tagsAdd.node).toEqual({
      id: productId,
      tags: ['existing', 'hermes-sale-live', 'hermes-summer-live'],
    });

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query TaggedReads($id: ID!) { product(id: $id) { id tags } products(first: 10, query: "tag:hermes-sale-live") { nodes { id tags } } productsCount(query: "tag:hermes-sale-live") { count precision } }',
        variables: { id: productId },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body.data.product).toEqual({
      id: productId,
      tags: ['existing', 'hermes-sale-live', 'hermes-summer-live'],
    });
    expect(queryResponse.body.data.products.nodes).toEqual([]);
    expect(queryResponse.body.data.productsCount).toEqual({
      count: 0,
      precision: 'EXACT',
    });

    store.markTagSearchLagged(productId, 0);
    const delayedQueryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query TaggedReads($id: ID!) { product(id: $id) { id tags } products(first: 10, query: "tag:hermes-sale-live") { nodes { id tags } } productsCount(query: "tag:hermes-sale-live") { count precision } }',
        variables: { id: productId },
      });

    expect(delayedQueryResponse.status).toBe(200);
    expect(delayedQueryResponse.body.data.product).toEqual({
      id: productId,
      tags: ['existing', 'hermes-sale-live', 'hermes-summer-live'],
    });
    expect(delayedQueryResponse.body.data.products.nodes).toEqual([
      {
        id: productId,
        tags: ['existing', 'hermes-sale-live', 'hermes-summer-live'],
      },
    ]);
    expect(delayedQueryResponse.body.data.productsCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });

    const removeTagsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation RemoveTags($id: ID!, $tags: [String!]!) { tagsRemove(id: $id, tags: $tags) { node { ... on Product { id tags } } userErrors { field message } } }',
        variables: {
          id: productId,
          tags: ['hermes-sale-live', 'missing'],
        },
      });

    expect(removeTagsResponse.status).toBe(200);
    expect(removeTagsResponse.body.data.tagsRemove.userErrors).toEqual([]);
    expect(removeTagsResponse.body.data.tagsRemove.node).toEqual({
      id: productId,
      tags: ['existing', 'hermes-summer-live'],
    });

    store.markTagSearchLagged(productId);
    const filteredResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query FilteredTaggedReads($id: ID!) { product(id: $id) { id tags } remaining: products(first: 10, query: "tag:hermes-summer-live") { nodes { id tags } } removed: products(first: 10, query: "tag:hermes-sale-live") { nodes { id } } remainingCount: productsCount(query: "tag:hermes-summer-live") { count precision } removedCount: productsCount(query: "tag:hermes-sale-live") { count precision } }',
        variables: { id: productId },
      });

    expect(filteredResponse.status).toBe(200);
    expect(filteredResponse.body.data.product).toEqual({
      id: productId,
      tags: ['existing', 'hermes-summer-live'],
    });
    expect(filteredResponse.body.data.remaining.nodes).toEqual([]);
    expect(filteredResponse.body.data.removed.nodes).toEqual([]);
    expect(filteredResponse.body.data.remainingCount).toEqual({
      count: 0,
      precision: 'EXACT',
    });
    expect(filteredResponse.body.data.removedCount).toEqual({
      count: 0,
      precision: 'EXACT',
    });
    expect(fetchSpy).toHaveBeenCalledTimes(4);
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

    const filteredResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
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

    const filteredProductsResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
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
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as { query?: string }) : {};
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

    const filteredProductsResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
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

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productUpdate(product: { id: "gid://shopify/Product/1", title: "Renamed Shirt" }) { product { id title } userErrors { field message } } }',
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "New Hat" }) { product { id title } userErrors { field message } } }',
    });

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productDelete(input: { id: "gid://shopify/Product/1" }) { deletedProductId userErrors { field message } } }',
    });

    const productsResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
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
      return new Response(
        JSON.stringify({
          data: { product: null, products: { nodes: [] }, productsCount: { count: 0, precision: 'EXACT' } },
        }),
        {
          status: 200,
          headers: { 'content-type': 'application/json' },
        },
      );
    });

    const app = createApp(config).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Status Hat", status: DRAFT }) { product { id status } userErrors { field message } } }',
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
        query:
          'query AfterStatusChange($id: ID!) { product(id: $id) { id status updatedAt } products(first: 10, query: "status:active") { nodes { id status } } activeCount: productsCount(query: "status:active") { count precision } }',
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

  it('returns a top-level GraphQL argument error when productChangeStatus receives a null literal productId', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('productChangeStatus null-id validation should not hit upstream fetch');
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    const invalidResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productChangeStatus(productId: null, status: ARCHIVED) { product { id status } userErrors { field message } } }',
    });

    expect(invalidResponse.status).toBe(200);
    expect(invalidResponse.body.data).toBeUndefined();
    expect(invalidResponse.body.errors).toEqual([
      expect.objectContaining({
        message:
          "Argument 'productId' on Field 'productChangeStatus' has an invalid value (null). Expected type 'ID!'.",
        path: ['mutation', 'productChangeStatus', 'productId'],
        extensions: expect.objectContaining({
          code: 'argumentLiteralsIncompatible',
          typeName: 'Field',
          argumentName: 'productId',
        }),
      }),
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('overlays productChangeStatus onto hydrated products and mirrors the live unknown-id validation slice', async () => {
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

    const unknownProductResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation ChangeStatus($productId: ID!, $status: ProductStatus!) { productChangeStatus(productId: $productId, status: $status) { product { id status updatedAt } userErrors { field message } } }',
        variables: {
          productId: 'gid://shopify/Product/999999999999999',
          status: 'ARCHIVED',
        },
      });

    expect(unknownProductResponse.status).toBe(200);
    expect(unknownProductResponse.body.data.productChangeStatus).toEqual({
      product: null,
      userErrors: [{ field: ['productId'], message: 'Product does not exist' }],
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

    const changeResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
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

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Publication Hat", status: DRAFT }) { product { id } userErrors { field message } } }',
    });

    const productId = createResponse.body.data.productCreate.product.id as string;

    const productIdPublishResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation PublishProductId($input: ProductPublishInput!) { productPublish(input: $input) { product { id } userErrors { field message } } }',
        variables: {
          input: {
            id: productId,
            productPublications: [{ publicationId: 'gid://shopify/Publication/1' }],
          },
        },
      });

    expect(productIdPublishResponse.status).toBe(200);
    expect(productIdPublishResponse.body.data.productPublish).toEqual({
      product: {
        id: productId,
      },
      userErrors: [],
    });

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

    const publishedQueryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query Published($id: ID!) { product(id: $id) { id publishedOnCurrentPublication availablePublicationsCount { count precision } resourcePublicationsCount { count precision } } }',
        variables: { id: productId },
      });

    expect(publishedQueryResponse.status).toBe(200);
    expect(publishedQueryResponse.body.data.product).toEqual(publishResponse.body.data.productPublish.product);

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
    expect(unpublishedQueryResponse.body.data.product).toEqual(unpublishResponse.body.data.productUnpublish.product);
    expect(fetchSpy).toHaveBeenCalledTimes(2);
  });

  it('stages generic publishable product roots locally without upstream writes', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('generic publishable product mutations should stage locally');
    });

    const app = createApp(config).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Generic Publishable Hat", status: ACTIVE }) { product { id } userErrors { field message } } }',
    });
    const productId = createResponse.body.data.productCreate.product.id as string;

    const publishResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation PublishGeneric($id: ID!, $input: [PublicationInput!]!) {
            publishablePublish(id: $id, input: $input) {
              publishable {
                ... on Product {
                  id
                  publishedOnCurrentPublication
                  availablePublicationsCount {
                    count
                    precision
                  }
                }
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          id: productId,
          input: [{ publicationId: 'gid://shopify/Publication/1' }],
        },
      });

    expect(publishResponse.status).toBe(200);
    expect(publishResponse.body.data.publishablePublish).toEqual({
      publishable: {
        id: productId,
        publishedOnCurrentPublication: true,
        availablePublicationsCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
      userErrors: [],
    });

    const unpublishResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation UnpublishGeneric($id: ID!, $input: [PublicationInput!]!) {
            publishableUnpublish(id: $id, input: $input) {
              publishable {
                ... on Product {
                  id
                  publishedOnCurrentPublication
                  availablePublicationsCount {
                    count
                    precision
                  }
                }
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          id: productId,
          input: [{ publicationId: 'gid://shopify/Publication/1' }],
        },
      });

    expect(unpublishResponse.status).toBe(200);
    expect(unpublishResponse.body.data.publishableUnpublish).toEqual({
      publishable: {
        id: productId,
        publishedOnCurrentPublication: false,
        availablePublicationsCount: {
          count: 0,
          precision: 'EXACT',
        },
      },
      userErrors: [],
    });

    const currentChannelPublishResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation PublishCurrentGeneric($id: ID!) {
            publishablePublishToCurrentChannel(id: $id) {
              publishable {
                ... on Product {
                  id
                  publishedOnCurrentPublication
                }
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { id: productId },
      });

    expect(currentChannelPublishResponse.status).toBe(200);
    expect(currentChannelPublishResponse.body.data.publishablePublishToCurrentChannel).toEqual({
      publishable: {
        id: productId,
        publishedOnCurrentPublication: true,
      },
      userErrors: [],
    });

    const currentChannelUnpublishResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation UnpublishCurrentGeneric($id: ID!) {
            publishableUnpublishToCurrentChannel(id: $id) {
              publishable {
                ... on Product {
                  id
                  publishedOnCurrentPublication
                }
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { id: productId },
      });

    expect(currentChannelUnpublishResponse.status).toBe(200);
    expect(currentChannelUnpublishResponse.body.data.publishableUnpublishToCurrentChannel).toEqual({
      publishable: {
        id: productId,
        publishedOnCurrentPublication: false,
      },
      userErrors: [],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('rejects unsupported generic publishable targets locally instead of proxying upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('unsupported generic publishable targets should not proxy upstream');
    });

    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `#graphql
          mutation PublishUnsupportedGeneric($id: ID!, $input: [PublicationInput!]!) {
            publishablePublish(id: $id, input: $input) {
              publishable {
                ... on Product {
                  id
                }
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: {
          id: 'gid://shopify/Article/1',
          input: [{ publicationId: 'gid://shopify/Publication/1' }],
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.publishablePublish).toEqual({
      publishable: null,
      userErrors: [{ field: ['id'], message: 'Only Product and Collection publishable IDs are supported locally' }],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('matches minimal productUnpublish payload selections without leaking unselected product fields', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('productUnpublish should stage locally without upstream fetches');
    });

    const app = createApp(config).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Minimal Unpublish Hat", status: DRAFT }) { product { id } userErrors { field message } } }',
    });

    const productId = createResponse.body.data.productCreate.product.id as string;

    const unpublishResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation Unpublish($input: ProductUnpublishInput!) { productUnpublish(input: $input) { userErrors { field message } } }',
        variables: {
          input: {
            id: productId,
            productPublications: [{ publicationId: 'gid://shopify/Publication/1' }],
          },
        },
      });

    expect(unpublishResponse.status).toBe(200);
    expect(unpublishResponse.body.data.productUnpublish).toEqual({
      userErrors: [],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
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

    const invalidPublishResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
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
    expect(overlayQueryResponse.body.data.product).toEqual(publishResponse.body.data.productPublish.product);
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

    const createProductResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Option Hat" }) { product { id } userErrors { field message } } }',
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
            hasVariants: true,
          },
        ],
      },
    ]);

    const colorOptionId = createOptionResponse.body.data.productOptionsCreate.product.options[0].id as string;
    const redValueId = createOptionResponse.body.data.productOptionsCreate.product.options[0].optionValues[0]
      .id as string;

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
        id: colorOptionId,
        name: 'Shade',
        position: 1,
        values: ['Crimson'],
        optionValues: [
          {
            id: redValueId,
            name: 'Crimson',
            hasVariants: true,
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
        id: expect.stringMatching(/^gid:\/\/shopify\/ProductOption\//),
        name: 'Title',
        position: 1,
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
      return new Response(
        JSON.stringify({
          data: { product: null, products: { nodes: [] }, productsCount: { count: 0, precision: 'EXACT' } },
        }),
        {
          status: 200,
          headers: { 'content-type': 'application/json' },
        },
      );
    });

    const app = createApp(config).callback();

    const createProductResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Variant Catalog Hat" }) { product { id } userErrors { field message } } }',
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
        compareAtPrice: '30.00',
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 6,
        inventoryItem: {
          id: expect.stringMatching(/^gid:\/\/shopify\/InventoryItem\//),
          tracked: true,
          requiresShipping: false,
        },
      },
    ]);
    expect(catalogResponse.body.data.products.nodes).toEqual([]);
    expect(catalogResponse.body.data.skuCount).toEqual({
      count: 0,
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
      count: 0,
      precision: 'EXACT',
    });
  });

  it('stages singular product variant create, update, and delete mutations locally via the same overlay model', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          data: { product: null, products: { nodes: [] }, productsCount: { count: 0, precision: 'EXACT' } },
        }),
        {
          status: 200,
          headers: { 'content-type': 'application/json' },
        },
      );
    });

    const app = createApp(config).callback();

    const createProductResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Single Variant Hat" }) { product { id } userErrors { field message } } }',
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
            inventoryQuantity: 0,
            selectedOptions: [],
            inventoryItem: {
              id: expect.stringMatching(/^gid:\/\/shopify\/InventoryItem\//),
              tracked: false,
              requiresShipping: true,
            },
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
    expect(downstreamQuery.body.data.products.nodes).toEqual([]);
    expect(downstreamQuery.body.data.skuCount).toEqual({
      count: 0,
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
      totalInventory: 0,
      tracksInventory: false,
      variants: {
        nodes: [
          {
            id: defaultVariantId,
            title: 'Default Title',
            sku: null,
            inventoryQuantity: 0,
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
      return new Response(
        JSON.stringify({
          data: { product: null, products: { nodes: [] }, productsCount: { count: 0, precision: 'EXACT' } },
        }),
        {
          status: 200,
          headers: { 'content-type': 'application/json' },
        },
      );
    });

    const app = createApp(config).callback();

    const createProductResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Option Synced Hat" }) { product { id } userErrors { field message } } }',
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
            { name: 'Red', hasVariants: true },
            { name: 'Blue', hasVariants: true },
          ],
        },
        {
          name: 'Size',
          values: ['Small', 'Large'],
          optionValues: [
            { name: 'Small', hasVariants: true },
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
            { name: 'Small', hasVariants: true },
            { name: 'Large', hasVariants: true },
          ],
        },
      ]),
    );

    const deleteVariantResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeleteVariant($id: ID!) { productVariantDelete(id: $id) { deletedProductVariantId userErrors { field message } } }',
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
            { name: 'Red', hasVariants: true },
            { name: 'Blue', hasVariants: false },
          ],
        },
        {
          name: 'Size',
          optionValues: [
            { name: 'Small', hasVariants: true },
            { name: 'Large', hasVariants: false },
          ],
        },
      ]),
    );
  });

  it('accepts Shopify-like bulk variant input shapes with optionValues, inventoryQuantities, and inventoryItem.sku', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          data: { product: null, products: { nodes: [] }, productsCount: { count: 0, precision: 'EXACT' } },
        }),
        {
          status: 200,
          headers: { 'content-type': 'application/json' },
        },
      );
    });

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createProductResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Live Shape Bulk Hat" }) { product { id } userErrors { field message } } }',
    });

    const productId = createProductResponse.body.data.productCreate.product.id as string;

    const createOptionsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateOptions($productId: ID!, $options: [OptionCreateInput!]!) { productOptionsCreate(productId: $productId, options: $options) { product { id options { name values optionValues { name hasVariants } } } userErrors { field message } } }',
        variables: {
          productId,
          options: [{ name: 'Color', values: [{ name: 'Red' }, { name: 'Blue' }] }],
        },
      });

    expect(createOptionsResponse.status).toBe(200);
    expect(createOptionsResponse.body.data.productOptionsCreate.userErrors).toEqual([]);

    const initialVariantQuery = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id variants(first: 10) { nodes { id title sku inventoryQuantity selectedOptions { name value } } } } }',
        variables: { id: productId },
      });

    const defaultVariantId = initialVariantQuery.body.data.product.variants.nodes[0].id as string;

    const invalidUpdateVariantsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateVariants($productId: ID!, $variants: [ProductVariantsBulkInput!]!) { productVariantsBulkUpdate(productId: $productId, variants: $variants) { product { id } productVariants { id sku inventoryQuantity } userErrors { field message } } }',
        variables: {
          productId,
          variants: [
            {
              id: defaultVariantId,
              inventoryQuantities: [{ availableQuantity: 4, locationId: 'gid://shopify/Location/1' }],
              inventoryItem: {
                sku: 'LIVE-BULK-RED',
                tracked: true,
                requiresShipping: true,
              },
            },
          ],
        },
      });

    expect(invalidUpdateVariantsResponse.status).toBe(200);
    expect(invalidUpdateVariantsResponse.body.data.productVariantsBulkUpdate.userErrors).toEqual([
      {
        field: ['variants', '0', 'inventoryQuantities'],
        message:
          'Inventory quantities can only be provided during create. To update inventory for existing variants, use inventoryAdjustQuantities.',
      },
    ]);

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
              barcode: '1111111111111',
              price: '24.00',
              compareAtPrice: '30.00',
              taxable: true,
              inventoryPolicy: 'DENY',
              inventoryItem: {
                sku: 'LIVE-BULK-RED',
                tracked: true,
                requiresShipping: true,
              },
            },
          ],
        },
      });

    expect(updateVariantsResponse.status).toBe(200);
    expect(updateVariantsResponse.body.data.productVariantsBulkUpdate.userErrors).toEqual([]);
    expect(updateVariantsResponse.body.data.productVariantsBulkUpdate.productVariants).toEqual([
      {
        id: defaultVariantId,
        title: 'Red',
        sku: 'LIVE-BULK-RED',
        barcode: '1111111111111',
        price: '24.00',
        compareAtPrice: '30.00',
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 0,
        inventoryItem: {
          id: expect.stringMatching(/^gid:\/\/shopify\/InventoryItem\//),
          tracked: true,
          requiresShipping: true,
        },
      },
    ]);

    const createVariantsResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateVariants($productId: ID!, $variants: [ProductVariantsBulkInput!]!) { productVariantsBulkCreate(productId: $productId, variants: $variants) { product { id totalInventory tracksInventory variants(first: 10) { nodes { id title sku barcode price inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } } productVariants { id title sku barcode price inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } userErrors { field message } } }',
        variables: {
          productId,
          variants: [
            {
              optionValues: [{ optionName: 'Color', name: 'Blue' }],
              barcode: '2222222222222',
              price: '26.00',
              inventoryQuantities: [{ availableQuantity: 6, locationId: 'gid://shopify/Location/1' }],
              inventoryItem: {
                sku: 'LIVE-BULK-BLUE',
                tracked: true,
                requiresShipping: false,
              },
            },
          ],
        },
      });

    expect(createVariantsResponse.status).toBe(200);
    expect(createVariantsResponse.body.data.productVariantsBulkCreate.userErrors).toEqual([]);
    expect(createVariantsResponse.body.data.productVariantsBulkCreate.productVariants).toEqual([
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/ProductVariant\//),
        title: 'Blue',
        sku: 'LIVE-BULK-BLUE',
        barcode: '2222222222222',
        price: '26.00',
        inventoryQuantity: 6,
        selectedOptions: [{ name: 'Color', value: 'Blue' }],
        inventoryItem: {
          id: expect.stringMatching(/^gid:\/\/shopify\/InventoryItem\//),
          tracked: true,
          requiresShipping: false,
        },
      },
    ]);

    const catalogResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id totalInventory tracksInventory variants(first: 10) { nodes { id title sku barcode price inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } } products(first: 10, query: "sku:LIVE-BULK-BLUE") { nodes { id totalInventory tracksInventory } } skuCount: productsCount(query: "sku:LIVE-BULK-BLUE") { count precision } }',
        variables: { id: productId },
      });

    expect(catalogResponse.status).toBe(200);
    expect(catalogResponse.body.data.product.totalInventory).toBe(6);
    expect(catalogResponse.body.data.product.tracksInventory).toBe(true);
    expect(catalogResponse.body.data.product.variants.nodes).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: defaultVariantId,
          title: 'Red',
          sku: 'LIVE-BULK-RED',
          inventoryQuantity: 0,
        }),
        expect.objectContaining({
          title: 'Blue',
          sku: 'LIVE-BULK-BLUE',
          inventoryQuantity: 6,
          selectedOptions: [{ name: 'Color', value: 'Blue' }],
        }),
      ]),
    );
    expect(catalogResponse.body.data.products.nodes).toEqual([]);
    expect(catalogResponse.body.data.skuCount).toEqual({
      count: 0,
      precision: 'EXACT',
    });
  });

  it('adds missing option values and updates hasVariants during bulk variant mutations', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      return new Response(JSON.stringify({ data: { product: null } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createProductResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Bulk Option Hat" }) { product { id } userErrors { field message } } }',
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

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Metafield Hat" }) { product { id } userErrors { field message } } }',
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

  it('upserts metafieldsSet against hydrated product metafields with product-scoped downstream replacement reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as { query?: string }) : {};
      const query = body.query ?? '';

      if (query.includes('product(id:')) {
        return new Response(
          JSON.stringify({
            data: {
              product: {
                id: 'gid://shopify/Product/2',
                title: 'Hydrated Metafields Hat',
                handle: 'hydrated-metafields-hat',
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
          {
            status: 200,
            headers: { 'content-type': 'application/json' },
          },
        );
      }

      throw new Error(`Unexpected fetch during metafieldsSet upsert test: ${String(input)}`);
    });

    const app = createApp(config).callback();

    const hydrateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id title handle status createdAt updatedAt metafields(first: 10) { edges { node { id namespace key type value } } } } }',
        variables: { id: 'gid://shopify/Product/2' },
      });

    expect(hydrateResponse.status).toBe(200);
    expect(hydrateResponse.body.data.product.metafields.edges).toHaveLength(2);
    const fetchCountBeforeMutation = fetchSpy.mock.calls.length;

    const setResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation SetMetafields($metafields: [MetafieldsSetInput!]!) { metafieldsSet(metafields: $metafields) { metafields { id namespace key type value } userErrors { field message } } }',
        variables: {
          metafields: [
            {
              ownerId: 'gid://shopify/Product/2',
              namespace: 'custom',
              key: 'material',
              type: 'single_line_text_field',
              value: 'Wool',
            },
            {
              ownerId: 'gid://shopify/Product/2',
              namespace: 'marketing',
              key: 'campaign',
              type: 'single_line_text_field',
              value: 'Spring',
            },
          ],
        },
      });

    expect(setResponse.status).toBe(200);
    expect(fetchSpy.mock.calls.length).toBe(fetchCountBeforeMutation);
    expect(setResponse.body.data.metafieldsSet).toEqual({
      metafields: [
        {
          id: 'gid://shopify/Metafield/9001',
          namespace: 'custom',
          key: 'material',
          type: 'single_line_text_field',
          value: 'Wool',
        },
        {
          id: expect.stringMatching(/^gid:\/\/shopify\/Metafield\//),
          namespace: 'marketing',
          key: 'campaign',
          type: 'single_line_text_field',
          value: 'Spring',
        },
      ],
      userErrors: [],
    });

    const newMetafield = setResponse.body.data.metafieldsSet.metafields[1];
    const overlayResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id primarySpec: metafield(namespace: "custom", key: "material") { id namespace key type value } origin: metafield(namespace: "details", key: "origin") { id namespace key type value } campaign: metafield(namespace: "marketing", key: "campaign") { id namespace key type value } metafields(first: 10) { nodes { id namespace key type value } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: 'gid://shopify/Product/2' },
      });

    expect(overlayResponse.status).toBe(200);
    expect(overlayResponse.body.data.product.primarySpec).toEqual({
      id: 'gid://shopify/Metafield/9001',
      namespace: 'custom',
      key: 'material',
      type: 'single_line_text_field',
      value: 'Wool',
    });
    expect(overlayResponse.body.data.product.origin).toEqual({
      id: 'gid://shopify/Metafield/9002',
      namespace: 'details',
      key: 'origin',
      type: 'single_line_text_field',
      value: 'VN',
    });
    expect(overlayResponse.body.data.product.campaign).toEqual(newMetafield);
    expect(overlayResponse.body.data.product.metafields).toEqual({
      nodes: [
        {
          id: 'gid://shopify/Metafield/9001',
          namespace: 'custom',
          key: 'material',
          type: 'single_line_text_field',
          value: 'Wool',
        },
        {
          id: 'gid://shopify/Metafield/9002',
          namespace: 'details',
          key: 'origin',
          type: 'single_line_text_field',
          value: 'VN',
        },
        newMetafield,
      ],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/Metafield/9001',
        endCursor: `cursor:${newMetafield.id}`,
      },
    });
  });

  it('rejects stale compareDigest metafieldsSet atomically against hydrated product metafields', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as { query?: string }) : {};
      const query = body.query ?? '';

      if (query.includes('product(id:')) {
        if (query.includes('primarySpec:')) {
          return new Response(
            JSON.stringify({
              data: {
                product: {
                  id: 'gid://shopify/Product/2',
                  primarySpec: {
                    value: 'Canvas',
                    compareDigest: 'live-digest',
                  },
                  campaign: null,
                  metafields: {
                    nodes: [
                      { namespace: 'custom', key: 'material', value: 'Canvas', compareDigest: 'live-digest' },
                      { namespace: 'details', key: 'origin', value: 'VN', compareDigest: 'origin-digest' },
                    ],
                  },
                },
              },
            }),
            {
              status: 200,
              headers: { 'content-type': 'application/json' },
            },
          );
        }

        return new Response(
          JSON.stringify({
            data: {
              product: {
                id: 'gid://shopify/Product/2',
                title: 'Hydrated Digest Hat',
                handle: 'hydrated-digest-hat',
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
                        compareDigest: 'live-digest',
                        jsonValue: 'Canvas',
                        createdAt: '2024-01-01T00:00:00.000Z',
                        updatedAt: '2024-01-02T00:00:00.000Z',
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
                        compareDigest: 'origin-digest',
                        jsonValue: 'VN',
                        createdAt: '2024-01-01T00:00:00.000Z',
                        updatedAt: '2024-01-02T00:00:00.000Z',
                        ownerType: 'PRODUCT',
                      },
                    },
                  ],
                },
              },
            },
          }),
          {
            status: 200,
            headers: { 'content-type': 'application/json' },
          },
        );
      }

      throw new Error(`Unexpected fetch during metafieldsSet compareDigest test: ${String(input)}`);
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    const hydrateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id metafields(first: 10) { edges { node { id namespace key type value compareDigest jsonValue createdAt updatedAt ownerType } } } } }',
        variables: { id: 'gid://shopify/Product/2' },
      });

    expect(hydrateResponse.status).toBe(200);
    expect(hydrateResponse.body.data.product.metafields.edges[0].node.compareDigest).toBe('live-digest');
    const fetchCountBeforeMutation = fetchSpy.mock.calls.length;

    const mutationQuery =
      'mutation SetMetafields($metafields: [MetafieldsSetInput!]!) { metafieldsSet(metafields: $metafields) { metafields { id namespace key type value compareDigest } userErrors { field message code elementIndex } } }';
    const staleResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: mutationQuery,
        variables: {
          metafields: [
            {
              ownerId: 'gid://shopify/Product/2',
              namespace: 'custom',
              key: 'material',
              type: 'single_line_text_field',
              value: 'Wool',
              compareDigest: 'stale-digest',
            },
            {
              ownerId: 'gid://shopify/Product/2',
              namespace: 'marketing',
              key: 'campaign',
              type: 'single_line_text_field',
              value: 'Spring',
            },
          ],
        },
      });

    expect(staleResponse.status).toBe(200);
    expect(fetchSpy.mock.calls.length).toBe(fetchCountBeforeMutation);
    expect(staleResponse.body.data.metafieldsSet).toEqual({
      metafields: [],
      userErrors: [
        {
          field: ['metafields', '0'],
          message:
            'The resource has been updated since it was loaded. Try again with an updated `compareDigest` value.',
          code: 'STALE_OBJECT',
          elementIndex: null,
        },
      ],
    });

    const overlayResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id primarySpec: metafield(namespace: "custom", key: "material") { value compareDigest } campaign: metafield(namespace: "marketing", key: "campaign") { value } metafields(first: 10) { nodes { namespace key value compareDigest } } } }',
        variables: { id: 'gid://shopify/Product/2' },
      });

    expect(overlayResponse.status).toBe(200);
    expect(overlayResponse.body.data.product.primarySpec).toEqual({
      value: 'Canvas',
      compareDigest: 'live-digest',
    });
    expect(overlayResponse.body.data.product.campaign).toBeNull();
    expect(overlayResponse.body.data.product.metafields.nodes).toEqual([
      { namespace: 'custom', key: 'material', value: 'Canvas', compareDigest: 'live-digest' },
      { namespace: 'details', key: 'origin', value: 'VN', compareDigest: 'origin-digest' },
    ]);

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.status).toBe(200);
    expect(logResponse.body.entries).toHaveLength(1);
    expect(logResponse.body.entries[0].requestBody).toEqual({
      query: mutationQuery,
      variables: {
        metafields: [
          {
            ownerId: 'gid://shopify/Product/2',
            namespace: 'custom',
            key: 'material',
            type: 'single_line_text_field',
            value: 'Wool',
            compareDigest: 'stale-digest',
          },
          {
            ownerId: 'gid://shopify/Product/2',
            namespace: 'marketing',
            key: 'campaign',
            type: 'single_line_text_field',
            value: 'Spring',
          },
        ],
      },
    });
  });

  it('stages metafieldDelete locally against hydrated product metafields and removes the deleted metafield from downstream reads', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as { query?: string }) : {};
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

  it('stages metafieldsDelete locally for mixed existing and missing product metafields with ordered null entries', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as { query?: string }) : {};
      const query = body.query ?? '';

      if (query.includes('product(id:')) {
        return new Response(
          JSON.stringify({
            data: {
              product: {
                id: 'gid://shopify/Product/2',
                title: 'Hydrated Metafields Delete Hat',
                handle: 'hydrated-metafields-delete-hat',
                status: 'ACTIVE',
                createdAt: '2024-01-01T00:00:00.000Z',
                updatedAt: '2024-01-02T00:00:00.000Z',
                metafields: {
                  edges: [
                    {
                      node: {
                        id: 'gid://shopify/Metafield/9101',
                        namespace: 'custom',
                        key: 'material',
                        type: 'single_line_text_field',
                        value: 'Canvas',
                      },
                    },
                    {
                      node: {
                        id: 'gid://shopify/Metafield/9102',
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

      throw new Error(`Unexpected fetch during metafieldsDelete test: ${String(input)}`);
    });

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    const hydrateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id metafields(first: 10) { edges { node { id namespace key type value } } } } }',
        variables: { id: 'gid://shopify/Product/2' },
      });

    expect(hydrateResponse.status).toBe(200);

    const deleteResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeleteMetafields($metafields: [MetafieldIdentifierInput!]!) { metafieldsDelete(metafields: $metafields) { deletedMetafields { key namespace ownerId } userErrors { field message } } }',
        variables: {
          metafields: [
            { ownerId: 'gid://shopify/Product/2', namespace: 'custom', key: 'material' },
            { ownerId: 'gid://shopify/Product/2', namespace: 'custom', key: 'missing' },
          ],
        },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.metafieldsDelete).toEqual({
      deletedMetafields: [
        {
          key: 'material',
          namespace: 'custom',
          ownerId: 'gid://shopify/Product/2',
        },
        null,
      ],
      userErrors: [],
    });

    const overlayResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id deletedSpec: metafield(namespace: "custom", key: "material") { id namespace key type value } missingSpec: metafield(namespace: "custom", key: "missing") { id namespace key type value } metafields(first: 10) { nodes { id namespace key type value } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: 'gid://shopify/Product/2' },
      });

    expect(overlayResponse.status).toBe(200);
    expect(overlayResponse.body.data.product.deletedSpec).toBeNull();
    expect(overlayResponse.body.data.product.missingSpec).toBeNull();
    expect(overlayResponse.body.data.product.metafields).toEqual({
      nodes: [
        {
          id: 'gid://shopify/Metafield/9102',
          namespace: 'details',
          key: 'origin',
          type: 'single_line_text_field',
          value: 'VN',
        },
      ],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/Metafield/9102',
        endCursor: 'cursor:gid://shopify/Metafield/9102',
      },
    });
  });

  it('matches captured metafieldsDelete empty input behavior', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeleteMetafields($metafields: [MetafieldIdentifierInput!]!) { metafieldsDelete(metafields: $metafields) { deletedMetafields { key namespace ownerId } userErrors { field message } } }',
        variables: { metafields: [] },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.metafieldsDelete).toEqual({
      deletedMetafields: [],
      userErrors: [],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns captured invalid-variable errors when metafieldsDelete identifier variables omit required fields', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeleteMetafields($metafields: [MetafieldIdentifierInput!]!) { metafieldsDelete(metafields: $metafields) { deletedMetafields { key namespace ownerId } userErrors { field message } } }',
        variables: {
          metafields: [{ ownerId: 'gid://shopify/Product/2', namespace: 'custom' }],
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.errors).toEqual([
      {
        message:
          'Variable $metafields of type [MetafieldIdentifierInput!]! was provided invalid value for 0.key (Expected value to not be null)',
        locations: [{ line: expect.any(Number), column: expect.any(Number) }],
        extensions: {
          code: 'INVALID_VARIABLE',
          value: [{ ownerId: 'gid://shopify/Product/2', namespace: 'custom' }],
          problems: [{ path: [0, 'key'], explanation: 'Expected value to not be null' }],
        },
      },
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns Shopify-like empty defaults in snapshot mode when no product exists', async () => {
    const snapshotApp = createApp({ ...config, readMode: 'snapshot' }).callback();

    const productResponse = await request(snapshotApp)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: 'query productById($id: ID!) { product(id: $id) { id title } }',
        variables: { id: 'gid://shopify/Product/404' },
      });

    const productsResponse = await request(snapshotApp).post('/admin/api/2025-01/graphql.json').send({
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
    expect(duplicateResponse.body.data.productDuplicate.newProduct.media.nodes).toEqual([]);
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
            nodes: [],
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

  it('deduplicates staged duplicate handles when productDuplicate is called twice with the same newTitle', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('productDuplicate should not hit upstream fetch');
    });
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/101',
        legacyResourceId: '101',
        title: 'Base Duplicate Hat',
        handle: 'base-duplicate-hat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'HERMES',
        productType: 'HATS',
        tags: [],
        totalInventory: null,
        tracksInventory: null,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const firstDuplicateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DuplicateProduct($productId: ID!, $newTitle: String!) { productDuplicate(productId: $productId, newTitle: $newTitle) { newProduct { id title handle status } userErrors { field message } } }',
        variables: {
          productId: 'gid://shopify/Product/101',
          newTitle: 'Copied Duplicate Hat',
        },
      });

    const secondDuplicateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DuplicateProduct($productId: ID!, $newTitle: String!) { productDuplicate(productId: $productId, newTitle: $newTitle) { newProduct { id title handle status } userErrors { field message } } }',
        variables: {
          productId: 'gid://shopify/Product/101',
          newTitle: 'Copied Duplicate Hat',
        },
      });

    expect(firstDuplicateResponse.status).toBe(200);
    expect(secondDuplicateResponse.status).toBe(200);
    expect(firstDuplicateResponse.body.data.productDuplicate.userErrors).toEqual([]);
    expect(secondDuplicateResponse.body.data.productDuplicate.userErrors).toEqual([]);
    expect(firstDuplicateResponse.body.data.productDuplicate.newProduct.handle).toBe('copied-duplicate-hat');
    expect(secondDuplicateResponse.body.data.productDuplicate.newProduct.handle).toBe('copied-duplicate-hat-1');
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages product media create, update, and delete locally with downstream media reads and inline fragment image fields', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createProductResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Media Hat" }) { product { id } userErrors { field message } } }',
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
          image: null,
        },
        image: null,
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
    expect(updateMediaResponse.body.data.productUpdateMedia.media).toEqual([]);
    expect(updateMediaResponse.body.data.productUpdateMedia.mediaUserErrors).toEqual([
      {
        field: ['media', '0', 'id'],
        message: 'Non-ready media cannot be updated.',
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
        alt: 'Front view',
        mediaContentType: 'IMAGE',
        status: 'PROCESSING',
        preview: {
          image: null,
        },
        image: null,
      },
    ]);

    const readyMediaResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query MediaReady($id: ID!) { product(id: $id) { id media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } } } }',
        variables: { id: productId },
      });

    expect(readyMediaResponse.status).toBe(200);
    expect(readyMediaResponse.body.data.product.media.nodes).toEqual([
      {
        id: mediaId,
        alt: 'Front view',
        mediaContentType: 'IMAGE',
        status: 'READY',
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

    const readyUpdateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateReadyMedia($productId: ID!, $media: [UpdateMediaInput!]!) { productUpdateMedia(productId: $productId, media: $media) { media { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } mediaUserErrors { field message } } }',
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

    expect(readyUpdateResponse.status).toBe(200);
    expect(readyUpdateResponse.body.data.productUpdateMedia.mediaUserErrors).toEqual([]);
    expect(readyUpdateResponse.body.data.productUpdateMedia.media).toEqual([
      {
        id: mediaId,
        alt: 'Updated front view',
        mediaContentType: 'IMAGE',
        status: 'READY',
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
    expect(deleteMediaResponse.body.data.productDeleteMedia.deletedProductImageIds).toEqual([
      expect.stringMatching(/^gid:\/\/shopify\/ProductImage\//),
    ]);
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

  it('deletes one staged product media item while preserving the remaining media connection', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createProductResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Two Media Hat" }) { product { id } userErrors { field message } } }',
    });

    const productId = createProductResponse.body.data.productCreate.product.id as string;
    const createMediaResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateMedia($productId: ID!, $media: [CreateMediaInput!]!) { productCreateMedia(productId: $productId, media: $media) { media { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } mediaUserErrors { field message } } }',
        variables: {
          productId,
          media: [
            {
              mediaContentType: 'IMAGE',
              originalSource: 'https://cdn.example.com/media-hat-front.jpg',
              alt: 'Front view',
            },
            {
              mediaContentType: 'IMAGE',
              originalSource: 'https://cdn.example.com/media-hat-back.jpg',
              alt: 'Back view',
            },
          ],
        },
      });

    expect(createMediaResponse.status).toBe(200);
    expect(createMediaResponse.body.data.productCreateMedia.mediaUserErrors).toEqual([]);
    const createdMedia = createMediaResponse.body.data.productCreateMedia.media as Array<{
      id: string;
    }>;
    expect(createdMedia).toHaveLength(2);
    const deletedMedia = createdMedia[0]!;
    const remainingMedia = createdMedia[1]!;

    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query PromoteProcessing($id: ID!) { product(id: $id) { id media(first: 10) { nodes { id status preview { image { url } } } } } }',
        variables: { id: productId },
      });
    await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query PromoteReady($id: ID!) { product(id: $id) { id media(first: 10) { nodes { id status preview { image { url } } } } } }',
        variables: { id: productId },
      });

    const deleteMediaResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeleteOneMedia($productId: ID!, $mediaIds: [ID!]!) { productDeleteMedia(productId: $productId, mediaIds: $mediaIds) { deletedMediaIds deletedProductImageIds mediaUserErrors { field message } product { id media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } } }',
        variables: {
          productId,
          mediaIds: [deletedMedia.id],
        },
      });

    expect(deleteMediaResponse.status).toBe(200);
    expect(deleteMediaResponse.body.data.productDeleteMedia).toMatchObject({
      deletedMediaIds: [deletedMedia.id],
      deletedProductImageIds: [expect.stringMatching(/^gid:\/\/shopify\/ProductImage\//)],
      mediaUserErrors: [],
    });
    expect(deleteMediaResponse.body.data.productDeleteMedia.product.media).toEqual({
      nodes: [
        {
          id: remainingMedia.id,
          alt: 'Back view',
          mediaContentType: 'IMAGE',
          status: 'READY',
          preview: {
            image: {
              url: 'https://cdn.example.com/media-hat-back.jpg',
            },
          },
          image: {
            url: 'https://cdn.example.com/media-hat-back.jpg',
          },
        },
      ],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: `cursor:${productId}:media:1`,
        endCursor: `cursor:${productId}:media:1`,
      },
    });

    const downstreamMediaQuery = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query MediaAfterPartialDelete($id: ID!) { product(id: $id) { id media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: productId },
      });

    expect(downstreamMediaQuery.status).toBe(200);
    expect(downstreamMediaQuery.body.data.product.media).toEqual(
      deleteMediaResponse.body.data.productDeleteMedia.product.media,
    );
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
            metafields: [{ namespace: 'custom', key: 'season', type: 'single_line_text_field', value: 'winter' }],
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
      tags: ['featured', 'winter'],
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
          'query ProductSetReadback($id: ID!) { product(id: $id) { id title descriptionHtml onlineStorePreviewUrl options { name values } variants(first: 10) { nodes { sku taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { measurement { weight { unit value } } } } } metafield(namespace: "custom", key: "season") { value } } total: productsCount(query: "sku:SNOW-SET-BLUE") { count precision } }',
        variables: { id: productId },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        product: {
          id: productId,
          title: 'Set Snowboard',
          descriptionHtml: '',
          onlineStorePreviewUrl: expect.stringContaining('https://shopify-draft-proxy.local/products_preview?'),
          options: [{ name: 'Color', values: ['Blue', 'Black'] }],
          variants: {
            nodes: [
              {
                sku: 'SNOW-SET-BLUE',
                taxable: true,
                inventoryPolicy: 'DENY',
                inventoryQuantity: 7,
                selectedOptions: [{ name: 'Color', value: 'Blue' }],
                inventoryItem: { measurement: { weight: { unit: 'KILOGRAMS', value: 0 } } },
              },
              {
                sku: 'SNOW-SET-BLACK',
                taxable: true,
                inventoryPolicy: 'DENY',
                inventoryQuantity: 3,
                selectedOptions: [{ name: 'Color', value: 'Black' }],
                inventoryItem: { measurement: { weight: { unit: 'KILOGRAMS', value: 0 } } },
              },
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

  it('auto-generates unique staged handles for synchronous productSet creates when titles collide', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('productSet should not hit upstream fetch');
    });

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const firstSetResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateViaProductSet($input: ProductSetInput!) { productSet(input: $input) { product { id title handle } productSetOperation { id status } userErrors { field message } } }',
        variables: {
          input: {
            title: 'Set Collision Board',
            status: 'DRAFT',
          },
        },
      });

    const secondSetResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateViaProductSet($input: ProductSetInput!) { productSet(input: $input) { product { id title handle } productSetOperation { id status } userErrors { field message } } }',
        variables: {
          input: {
            title: 'Set Collision Board',
            status: 'DRAFT',
          },
        },
      });

    expect(firstSetResponse.status).toBe(200);
    expect(secondSetResponse.status).toBe(200);
    expect(firstSetResponse.body.data.productSet.userErrors).toEqual([]);
    expect(secondSetResponse.body.data.productSet.userErrors).toEqual([]);
    expect(firstSetResponse.body.data.productSet.product.handle).toBe('set-collision-board');
    expect(secondSetResponse.body.data.productSet.product.handle).toBe('set-collision-board-1');
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('normalizes explicit productSet handles before storing them', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('productSet should not hit upstream fetch');
    });

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const createResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Set Normalized Handle Owner',
          },
        },
      });

    const productId = createResponse.body.data.productCreate.product.id as string;

    const setResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateViaProductSet($identifier: ProductSetIdentifiers, $input: ProductSetInput!, $synchronous: Boolean!) { productSet(identifier: $identifier, input: $input, synchronous: $synchronous) { product { id title handle } productSetOperation { id status } userErrors { field message } } }',
        variables: {
          synchronous: true,
          identifier: { id: productId },
          input: {
            title: 'Set Normalized Handle Owner',
            handle: '  Another Weird/Handle 300 % ',
            status: 'DRAFT',
          },
        },
      });

    expect(setResponse.status).toBe(200);
    expect(setResponse.body.data.productSet).toEqual({
      product: {
        id: productId,
        title: 'Set Normalized Handle Owner',
        handle: 'another-weird-handle-300',
      },
      productSetOperation: null,
      userErrors: [],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('preserves the current handle when productSet tries to claim a different products explicit handle', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('productSet should not hit upstream fetch');
    });

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();

    const firstCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Set Handle Owner',
          },
        },
      });
    const secondCreateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateProduct($product: ProductCreateInput!) { productCreate(product: $product) { product { id title handle } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Set Handle Challenger',
          },
        },
      });

    const firstHandle = firstCreateResponse.body.data.productCreate.product.handle as string;
    const secondProductId = secondCreateResponse.body.data.productCreate.product.id as string;
    const secondHandle = secondCreateResponse.body.data.productCreate.product.handle as string;

    const setResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateViaProductSet($identifier: ProductSetIdentifiers, $input: ProductSetInput!, $synchronous: Boolean!) { productSet(identifier: $identifier, input: $input, synchronous: $synchronous) { product { id title handle } productSetOperation { id status } userErrors { field message } } }',
        variables: {
          synchronous: true,
          identifier: { id: secondProductId },
          input: {
            handle: firstHandle,
            status: 'DRAFT',
          },
        },
      });

    expect(setResponse.status).toBe(200);
    expect(setResponse.body.data.productSet).toEqual({
      product: {
        id: secondProductId,
        title: 'Set Handle Challenger',
        handle: secondHandle,
      },
      productSetOperation: null,
      userErrors: [
        { field: ['input', 'handle'], message: `Handle '${firstHandle}' already in use. Please provide a new handle.` },
      ],
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
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/70001',
          tracked: true,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
        },
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
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/70002',
          tracked: true,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
        },
      },
    ]);
    store.replaceBaseCollectionsForProduct('gid://shopify/Product/700', [
      { id: 'gid://shopify/Collection/1', productId: 'gid://shopify/Product/700', title: 'Winter', handle: 'winter' },
      { id: 'gid://shopify/Collection/2', productId: 'gid://shopify/Product/700', title: 'Sale', handle: 'sale' },
    ]);
    store.replaceBaseMetafieldsForProduct('gid://shopify/Product/700', [
      {
        id: 'gid://shopify/Metafield/7001',
        productId: 'gid://shopify/Product/700',
        namespace: 'custom',
        key: 'season',
        type: 'single_line_text_field',
        value: 'old',
      },
      {
        id: 'gid://shopify/Metafield/7002',
        productId: 'gid://shopify/Product/700',
        namespace: 'custom',
        key: 'material',
        type: 'single_line_text_field',
        value: 'wood',
      },
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
            metafields: [{ namespace: 'custom', key: 'season', type: 'single_line_text_field', value: 'new' }],
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
              values: ['Blue'],
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

  it('stages inventoryAdjustQuantities locally with Shopify-like aggregate inventory lag', async () => {
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
          quantityAfterChange: null,
          item: { id: 'gid://shopify/InventoryItem/8001' },
          location: { id: 'gid://shopify/Location/1' },
        },
        {
          name: 'available',
          delta: 4,
          quantityAfterChange: null,
          item: { id: 'gid://shopify/InventoryItem/8002' },
          location: { id: 'gid://shopify/Location/1' },
        },
        {
          name: 'on_hand',
          delta: -2,
          quantityAfterChange: null,
          item: { id: 'gid://shopify/InventoryItem/8001' },
          location: { id: 'gid://shopify/Location/1' },
        },
        {
          name: 'on_hand',
          delta: 4,
          quantityAfterChange: null,
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
          totalInventory: 12,
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
              totalInventory: 12,
            },
          },
        },
        matching: {
          nodes: [],
        },
        matchingCount: { count: 0, precision: 'EXACT' },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns the captured staffMember access error while preserving inventoryAdjustQuantities payload data', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventoryAdjustQuantities should not hit upstream fetch');
    });

    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/809',
        legacyResourceId: '809',
        title: 'Inventory Staff Audit Tee',
        handle: 'inventory-staff-audit-tee',
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
    store.replaceBaseVariantsForProduct('gid://shopify/Product/809', [
      {
        id: 'gid://shopify/ProductVariant/8091',
        productId: 'gid://shopify/Product/809',
        title: 'Default Title',
        sku: 'INV-STAFF',
        barcode: null,
        price: '25.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 6,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8091',
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
        query: `mutation AdjustInventory($input: InventoryAdjustQuantitiesInput!) {
          inventoryAdjustQuantities(input: $input) {
            inventoryAdjustmentGroup {
              staffMember { id }
              changes { name delta item { id } location { id } }
            }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            name: 'available',
            reason: 'correction',
            changes: [
              {
                inventoryItemId: 'gid://shopify/InventoryItem/8091',
                locationId: 'gid://shopify/Location/1',
                delta: 1,
              },
            ],
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      errors: [
        {
          message:
            'Access denied for staffMember field. Required access: `read_users` access scope. Also: The app must be a finance embedded app or installed on a Shopify Plus or Advanced store. Contact Shopify Support to enable this scope for your app.',
          locations: [{ line: 4, column: 15 }],
          extensions: {
            code: 'ACCESS_DENIED',
            documentation: 'https://shopify.dev/api/usage/access-scopes',
            requiredAccess:
              '`read_users` access scope. Also: The app must be a finance embedded app or installed on a Shopify Plus or Advanced store. Contact Shopify Support to enable this scope for your app.',
          },
          path: ['inventoryAdjustQuantities', 'inventoryAdjustmentGroup', 'staffMember'],
        },
      ],
      data: {
        inventoryAdjustQuantities: {
          inventoryAdjustmentGroup: {
            staffMember: null,
            changes: [
              {
                name: 'available',
                delta: 1,
                item: { id: 'gid://shopify/InventoryItem/8091' },
                location: { id: 'gid://shopify/Location/1' },
              },
              {
                name: 'on_hand',
                delta: 1,
                item: { id: 'gid://shopify/InventoryItem/8091' },
                location: { id: 'gid://shopify/Location/1' },
              },
            ],
          },
          userErrors: [],
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages inventoryItemUpdate locally and keeps downstream inventory item reads aligned', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventoryItemUpdate should not hit upstream fetch');
    });

    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/806',
        legacyResourceId: '806',
        title: 'Inventory Metadata Coat',
        handle: 'inventory-metadata-coat',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'ACME',
        productType: 'OUTERWEAR',
        tags: ['inventory'],
        totalInventory: 4,
        tracksInventory: false,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseVariantsForProduct('gid://shopify/Product/806', [
      {
        id: 'gid://shopify/ProductVariant/8061',
        productId: 'gid://shopify/Product/806',
        title: 'Default Title',
        sku: 'COAT-BASE',
        barcode: null,
        price: '120.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 4,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8061',
          tracked: false,
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
          inventoryLevels: null,
        },
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();
    const mutationResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateInventoryItem($id: ID!, $input: InventoryItemInput!) { inventoryItemUpdate(id: $id, input: $input) { inventoryItem { id tracked requiresShipping countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode measurement { weight { unit value } } variant { id inventoryQuantity product { id title tracksInventory } } } userErrors { field message } } }',
        variables: {
          id: 'gid://shopify/InventoryItem/8061',
          input: {
            tracked: true,
            requiresShipping: false,
            countryCodeOfOrigin: 'CA',
            provinceCodeOfOrigin: 'ON',
            harmonizedSystemCode: '620343',
            measurement: {
              weight: {
                unit: 'KILOGRAMS',
                value: 2.5,
              },
            },
          },
        },
      });

    expect(mutationResponse.status).toBe(200);
    expect(mutationResponse.body).toEqual({
      data: {
        inventoryItemUpdate: {
          inventoryItem: {
            id: 'gid://shopify/InventoryItem/8061',
            tracked: true,
            requiresShipping: false,
            countryCodeOfOrigin: 'CA',
            provinceCodeOfOrigin: 'ON',
            harmonizedSystemCode: '620343',
            measurement: {
              weight: {
                unit: 'KILOGRAMS',
                value: 2.5,
              },
            },
            variant: {
              id: 'gid://shopify/ProductVariant/8061',
              inventoryQuantity: 4,
              product: {
                id: 'gid://shopify/Product/806',
                title: 'Inventory Metadata Coat',
                tracksInventory: true,
              },
            },
          },
          userErrors: [],
        },
      },
    });

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query InspectInventoryItem($variantId: ID!, $inventoryItemId: ID!) { variant: productVariant(id: $variantId) { id inventoryQuantity inventoryItem { id tracked requiresShipping countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode measurement { weight { unit value } } } } stock: inventoryItem(id: $inventoryItemId) { id tracked requiresShipping countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode measurement { weight { unit value } } variant { id inventoryQuantity product { id title tracksInventory } } } }',
        variables: {
          variantId: 'gid://shopify/ProductVariant/8061',
          inventoryItemId: 'gid://shopify/InventoryItem/8061',
        },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        variant: {
          id: 'gid://shopify/ProductVariant/8061',
          inventoryQuantity: 4,
          inventoryItem: {
            id: 'gid://shopify/InventoryItem/8061',
            tracked: true,
            requiresShipping: false,
            countryCodeOfOrigin: 'CA',
            provinceCodeOfOrigin: 'ON',
            harmonizedSystemCode: '620343',
            measurement: {
              weight: {
                unit: 'KILOGRAMS',
                value: 2.5,
              },
            },
          },
        },
        stock: {
          id: 'gid://shopify/InventoryItem/8061',
          tracked: true,
          requiresShipping: false,
          countryCodeOfOrigin: 'CA',
          provinceCodeOfOrigin: 'ON',
          harmonizedSystemCode: '620343',
          measurement: {
            weight: {
              unit: 'KILOGRAMS',
              value: 2.5,
            },
          },
          variant: {
            id: 'gid://shopify/ProductVariant/8061',
            inventoryQuantity: 4,
            product: {
              id: 'gid://shopify/Product/806',
              title: 'Inventory Metadata Coat',
              tracksInventory: true,
            },
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors the captured inventoryItemUpdate unknown-id userError in snapshot mode', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventoryItemUpdate validation should not hit upstream fetch');
    });

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation UpdateInventoryItem($id: ID!, $input: InventoryItemInput!) { inventoryItemUpdate(id: $id, input: $input) { inventoryItem { id } userErrors { field message } } }',
        variables: {
          id: 'gid://shopify/InventoryItem/99999999999999',
          input: {
            tracked: true,
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        inventoryItemUpdate: {
          inventoryItem: null,
          userErrors: [
            {
              field: ['id'],
              message: "The product couldn't be updated because it does not exist.",
            },
          ],
        },
      },
    });
  });

  it('stages incoming inventory adjustments locally without changing available inventory aggregates', async () => {
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
          'mutation AdjustInventory($input: InventoryAdjustQuantitiesInput!) { inventoryAdjustQuantities(input: $input) { inventoryAdjustmentGroup { id app { title handle } referenceDocumentUri changes { name delta quantityAfterChange ledgerDocumentUri item { id } location { id name } } } userErrors { field message } } }',
        variables: {
          input: {
            name: 'incoming',
            reason: 'correction',
            referenceDocumentUri: 'logistics://incoming/2026-04-17',
            changes: [
              {
                inventoryItemId: 'gid://shopify/InventoryItem/8011',
                locationId: 'gid://shopify/Location/1',
                ledgerDocumentUri: 'ledger://incoming/8011',
                delta: 3,
              },
            ],
          },
        },
      });

    expect(mutationResponse.status).toBe(200);
    expect(mutationResponse.body).toEqual({
      data: {
        inventoryAdjustQuantities: {
          inventoryAdjustmentGroup: {
            id: expect.any(String),
            app: {
              title: null,
              handle: null,
            },
            referenceDocumentUri: 'logistics://incoming/2026-04-17',
            changes: [
              {
                name: 'incoming',
                delta: 3,
                quantityAfterChange: null,
                ledgerDocumentUri: 'ledger://incoming/8011',
                item: { id: 'gid://shopify/InventoryItem/8011' },
                location: { id: 'gid://shopify/Location/1', name: null },
              },
            ],
          },
          userErrors: [],
        },
      },
    });

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($productId: ID!, $variantId: ID!, $inventoryItemId: ID!) { product(id: $productId) { id totalInventory tracksInventory variants(first: 10) { nodes { id inventoryQuantity inventoryItem { id inventoryLevels(first: 5) { nodes { quantities(names: ["available", "incoming", "reserved", "damaged", "quality_control", "safety_stock", "committed", "on_hand"]) { name quantity updatedAt } } } } } } } variant: productVariant(id: $variantId) { id inventoryQuantity inventoryItem { id inventoryLevels(first: 5) { nodes { quantities(names: ["available", "incoming", "reserved", "damaged", "quality_control", "safety_stock", "committed", "on_hand"]) { name quantity updatedAt } } } } } stock: inventoryItem(id: $inventoryItemId) { id inventoryLevels(first: 5) { nodes { quantities(names: ["available", "incoming", "reserved", "damaged", "quality_control", "safety_stock", "committed", "on_hand"]) { name quantity updatedAt } } } } }',
        variables: {
          productId: 'gid://shopify/Product/801',
          variantId: 'gid://shopify/ProductVariant/8011',
          inventoryItemId: 'gid://shopify/InventoryItem/8011',
        },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        product: {
          id: 'gid://shopify/Product/801',
          totalInventory: 6,
          tracksInventory: true,
          variants: {
            nodes: [
              {
                id: 'gid://shopify/ProductVariant/8011',
                inventoryQuantity: 6,
                inventoryItem: {
                  id: 'gid://shopify/InventoryItem/8011',
                  inventoryLevels: {
                    nodes: [
                      {
                        quantities: [
                          { name: 'available', quantity: 6, updatedAt: expect.any(String) },
                          { name: 'incoming', quantity: 3, updatedAt: expect.any(String) },
                          { name: 'reserved', quantity: 0, updatedAt: null },
                          { name: 'damaged', quantity: 0, updatedAt: null },
                          { name: 'quality_control', quantity: 0, updatedAt: null },
                          { name: 'safety_stock', quantity: 0, updatedAt: null },
                          { name: 'committed', quantity: 0, updatedAt: null },
                          { name: 'on_hand', quantity: 6, updatedAt: null },
                        ],
                      },
                    ],
                  },
                },
              },
            ],
          },
        },
        variant: {
          id: 'gid://shopify/ProductVariant/8011',
          inventoryQuantity: 6,
          inventoryItem: {
            id: 'gid://shopify/InventoryItem/8011',
            inventoryLevels: {
              nodes: [
                {
                  quantities: [
                    { name: 'available', quantity: 6, updatedAt: expect.any(String) },
                    { name: 'incoming', quantity: 3, updatedAt: expect.any(String) },
                    { name: 'reserved', quantity: 0, updatedAt: null },
                    { name: 'damaged', quantity: 0, updatedAt: null },
                    { name: 'quality_control', quantity: 0, updatedAt: null },
                    { name: 'safety_stock', quantity: 0, updatedAt: null },
                    { name: 'committed', quantity: 0, updatedAt: null },
                    { name: 'on_hand', quantity: 6, updatedAt: null },
                  ],
                },
              ],
            },
          },
        },
        stock: {
          id: 'gid://shopify/InventoryItem/8011',
          inventoryLevels: {
            nodes: [
              {
                quantities: [
                  { name: 'available', quantity: 6, updatedAt: expect.any(String) },
                  { name: 'incoming', quantity: 3, updatedAt: expect.any(String) },
                  { name: 'reserved', quantity: 0, updatedAt: null },
                  { name: 'damaged', quantity: 0, updatedAt: null },
                  { name: 'quality_control', quantity: 0, updatedAt: null },
                  { name: 'safety_stock', quantity: 0, updatedAt: null },
                  { name: 'committed', quantity: 0, updatedAt: null },
                  { name: 'on_hand', quantity: 6, updatedAt: null },
                ],
              },
            ],
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages all captured non-available inventory quantity names with downstream level visibility', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventoryAdjustQuantities should not hit upstream fetch');
    });

    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/810',
        legacyResourceId: '810',
        title: 'Inventory Ledger Tee',
        handle: 'inventory-ledger-tee',
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
    store.replaceBaseVariantsForProduct('gid://shopify/Product/810', [
      {
        id: 'gid://shopify/ProductVariant/8101',
        productId: 'gid://shopify/Product/810',
        title: 'Default Title',
        sku: 'INV-LEDGER',
        barcode: null,
        price: '20.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 6,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8101',
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
    const quantityNames = ['damaged', 'quality_control', 'reserved', 'safety_stock'];
    for (const [index, name] of quantityNames.entries()) {
      const response = await request(app)
        .post('/admin/api/2025-01/graphql.json')
        .send({
          query:
            'mutation AdjustInventory($input: InventoryAdjustQuantitiesInput!) { inventoryAdjustQuantities(input: $input) { inventoryAdjustmentGroup { changes { name delta ledgerDocumentUri item { id } location { id } } } userErrors { field message } } }',
          variables: {
            input: {
              name,
              reason: 'correction',
              referenceDocumentUri: `logistics://ledger/${name}`,
              changes: [
                {
                  inventoryItemId: 'gid://shopify/InventoryItem/8101',
                  locationId: 'gid://shopify/Location/1',
                  ledgerDocumentUri: `ledger://${name}/8101`,
                  delta: index + 1,
                },
              ],
            },
          },
        });

      expect(response.status).toBe(200);
      expect(response.body).toEqual({
        data: {
          inventoryAdjustQuantities: {
            inventoryAdjustmentGroup: {
              changes: [
                {
                  name,
                  delta: index + 1,
                  ledgerDocumentUri: `ledger://${name}/8101`,
                  item: { id: 'gid://shopify/InventoryItem/8101' },
                  location: { id: 'gid://shopify/Location/1' },
                },
              ],
            },
            userErrors: [],
          },
        },
      });
    }

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($productId: ID!, $variantId: ID!, $inventoryItemId: ID!) { product(id: $productId) { id totalInventory variants(first: 10) { nodes { id inventoryQuantity inventoryItem { id inventoryLevels(first: 5) { nodes { quantities(names: ["available", "damaged", "quality_control", "reserved", "safety_stock", "on_hand"]) { name quantity updatedAt } } } } } } } variant: productVariant(id: $variantId) { id inventoryQuantity inventoryItem { id inventoryLevels(first: 5) { nodes { quantities(names: ["available", "damaged", "quality_control", "reserved", "safety_stock", "on_hand"]) { name quantity updatedAt } } } } } stock: inventoryItem(id: $inventoryItemId) { id inventoryLevels(first: 5) { nodes { quantities(names: ["available", "damaged", "quality_control", "reserved", "safety_stock", "on_hand"]) { name quantity updatedAt } } } } matching: products(first: 10, query: "inventory_total:>=7") { nodes { id totalInventory } } matchingCount: productsCount(query: "inventory_total:>=7") { count precision } }',
        variables: {
          productId: 'gid://shopify/Product/810',
          variantId: 'gid://shopify/ProductVariant/8101',
          inventoryItemId: 'gid://shopify/InventoryItem/8101',
        },
      });

    const expectedQuantities = [
      { name: 'available', quantity: 6, updatedAt: expect.any(String) },
      { name: 'damaged', quantity: 1, updatedAt: expect.any(String) },
      { name: 'quality_control', quantity: 2, updatedAt: expect.any(String) },
      { name: 'reserved', quantity: 3, updatedAt: expect.any(String) },
      { name: 'safety_stock', quantity: 4, updatedAt: expect.any(String) },
      { name: 'on_hand', quantity: 6, updatedAt: null },
    ];
    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        product: {
          id: 'gid://shopify/Product/810',
          totalInventory: 6,
          variants: {
            nodes: [
              {
                id: 'gid://shopify/ProductVariant/8101',
                inventoryQuantity: 6,
                inventoryItem: {
                  id: 'gid://shopify/InventoryItem/8101',
                  inventoryLevels: { nodes: [{ quantities: expectedQuantities }] },
                },
              },
            ],
          },
        },
        variant: {
          id: 'gid://shopify/ProductVariant/8101',
          inventoryQuantity: 6,
          inventoryItem: {
            id: 'gid://shopify/InventoryItem/8101',
            inventoryLevels: { nodes: [{ quantities: expectedQuantities }] },
          },
        },
        stock: {
          id: 'gid://shopify/InventoryItem/8101',
          inventoryLevels: { nodes: [{ quantities: expectedQuantities }] },
        },
        matching: { nodes: [] },
        matchingCount: { count: 0, precision: 'EXACT' },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('replays inventory adjustment app identity including app id and api key when configured', async () => {
    const previousHandle = process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'];
    const previousApiKey = process.env['SHOPIFY_CONFORMANCE_APP_API_KEY'];
    const previousAppId = process.env['SHOPIFY_CONFORMANCE_APP_ID'];

    process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] = 'hermes-conformance-products';
    process.env['SHOPIFY_CONFORMANCE_APP_API_KEY'] = '0db6d7e08e4ba05ce97440df36c7ed33';
    process.env['SHOPIFY_CONFORMANCE_APP_ID'] = 'gid://shopify/App/347082227713';

    try {
      const fetchSpy = vi.spyOn(globalThis, 'fetch');
      store.upsertBaseProducts([
        {
          id: 'gid://shopify/Product/803',
          legacyResourceId: '803',
          title: 'Inventory Gloves',
          handle: 'inventory-gloves',
          status: 'ACTIVE',
          publicationIds: [],
          createdAt: '2024-01-01T00:00:00.000Z',
          updatedAt: '2024-01-02T00:00:00.000Z',
          vendor: 'ACME',
          productType: 'ACCESSORIES',
          tags: ['inventory'],
          totalInventory: 2,
          tracksInventory: true,
          descriptionHtml: null,
          onlineStorePreviewUrl: null,
          templateSuffix: null,
          seo: { title: null, description: null },
          category: null,
        },
      ]);
      store.replaceBaseVariantsForProduct('gid://shopify/Product/803', [
        {
          id: 'gid://shopify/ProductVariant/8031',
          productId: 'gid://shopify/Product/803',
          title: 'Default Title',
          sku: 'INV-GLOVES',
          barcode: null,
          price: '25.00',
          compareAtPrice: null,
          taxable: true,
          inventoryPolicy: 'DENY',
          inventoryQuantity: 2,
          selectedOptions: [],
          inventoryItem: {
            id: 'gid://shopify/InventoryItem/8031',
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
            'mutation AdjustInventory($input: InventoryAdjustQuantitiesInput!) { inventoryAdjustQuantities(input: $input) { inventoryAdjustmentGroup { id app { id title apiKey handle } changes { name delta item { id } location { id } } } userErrors { field message } } }',
          variables: {
            input: {
              name: 'available',
              reason: 'correction',
              referenceDocumentUri: 'logistics://gloves/2026-04-17',
              changes: [
                {
                  inventoryItemId: 'gid://shopify/InventoryItem/8031',
                  locationId: 'gid://shopify/Location/1',
                  delta: 1,
                },
              ],
            },
          },
        });

      expect(mutationResponse.status).toBe(200);
      expect(mutationResponse.body).toEqual({
        data: {
          inventoryAdjustQuantities: {
            inventoryAdjustmentGroup: {
              id: expect.any(String),
              app: {
                id: 'gid://shopify/App/347082227713',
                title: 'hermes-conformance-products',
                apiKey: '0db6d7e08e4ba05ce97440df36c7ed33',
                handle: 'hermes-conformance-products',
              },
              changes: [
                {
                  name: 'available',
                  delta: 1,
                  item: { id: 'gid://shopify/InventoryItem/8031' },
                  location: { id: 'gid://shopify/Location/1' },
                },
                {
                  name: 'on_hand',
                  delta: 1,
                  item: { id: 'gid://shopify/InventoryItem/8031' },
                  location: { id: 'gid://shopify/Location/1' },
                },
              ],
            },
            userErrors: [],
          },
        },
      });
      expect(fetchSpy).not.toHaveBeenCalled();
    } finally {
      if (previousHandle === undefined) {
        delete process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'];
      } else {
        process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] = previousHandle;
      }
      if (previousApiKey === undefined) {
        delete process.env['SHOPIFY_CONFORMANCE_APP_API_KEY'];
      } else {
        process.env['SHOPIFY_CONFORMANCE_APP_API_KEY'] = previousApiKey;
      }
      if (previousAppId === undefined) {
        delete process.env['SHOPIFY_CONFORMANCE_APP_ID'];
      } else {
        process.env['SHOPIFY_CONFORMANCE_APP_ID'] = previousAppId;
      }
    }
  });

  it('keeps available inventory adjustments scoped per location while variant totals sum across levels', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/804',
        legacyResourceId: '804',
        title: 'Inventory Jacket',
        handle: 'inventory-jacket',
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
    store.replaceBaseVariantsForProduct('gid://shopify/Product/804', [
      {
        id: 'gid://shopify/ProductVariant/8041',
        productId: 'gid://shopify/Product/804',
        title: 'Default Title',
        sku: 'INV-JACKET',
        barcode: null,
        price: '70.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 6,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8041',
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
          'mutation AdjustInventory($input: InventoryAdjustQuantitiesInput!) { inventoryAdjustQuantities(input: $input) { inventoryAdjustmentGroup { changes { name delta item { id } location { id } } } userErrors { field message } } }',
        variables: {
          input: {
            name: 'available',
            reason: 'correction',
            referenceDocumentUri: 'logistics://rebalancing/2026-04-17',
            changes: [
              {
                inventoryItemId: 'gid://shopify/InventoryItem/8041',
                locationId: 'gid://shopify/Location/1',
                delta: -2,
              },
              {
                inventoryItemId: 'gid://shopify/InventoryItem/8041',
                locationId: 'gid://shopify/Location/2',
                delta: 4,
              },
            ],
          },
        },
      });

    expect(mutationResponse.status).toBe(200);
    expect(mutationResponse.body.data.inventoryAdjustQuantities.userErrors).toEqual([]);

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($variantId: ID!, $inventoryItemId: ID!) { variant: productVariant(id: $variantId) { id inventoryQuantity inventoryItem { id inventoryLevels(first: 10) { nodes { location { id } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } } } } } stock: inventoryItem(id: $inventoryItemId) { id inventoryLevels(first: 10) { nodes { location { id } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } } } } }',
        variables: {
          variantId: 'gid://shopify/ProductVariant/8041',
          inventoryItemId: 'gid://shopify/InventoryItem/8041',
        },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        variant: {
          id: 'gid://shopify/ProductVariant/8041',
          inventoryQuantity: 8,
          inventoryItem: {
            id: 'gid://shopify/InventoryItem/8041',
            inventoryLevels: {
              nodes: [
                {
                  location: { id: 'gid://shopify/Location/1' },
                  quantities: [
                    { name: 'available', quantity: 4 },
                    { name: 'on_hand', quantity: 4 },
                    { name: 'incoming', quantity: 0 },
                  ],
                },
                {
                  location: { id: 'gid://shopify/Location/2' },
                  quantities: [
                    { name: 'available', quantity: 4 },
                    { name: 'on_hand', quantity: 4 },
                    { name: 'incoming', quantity: 0 },
                  ],
                },
              ],
            },
          },
        },
        stock: {
          id: 'gid://shopify/InventoryItem/8041',
          inventoryLevels: {
            nodes: [
              {
                location: { id: 'gid://shopify/Location/1' },
                quantities: [
                  { name: 'available', quantity: 4 },
                  { name: 'on_hand', quantity: 4 },
                  { name: 'incoming', quantity: 0 },
                ],
              },
              {
                location: { id: 'gid://shopify/Location/2' },
                quantities: [
                  { name: 'available', quantity: 4 },
                  { name: 'on_hand', quantity: 4 },
                  { name: 'incoming', quantity: 0 },
                ],
              },
            ],
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('keeps non-available inventory adjustments scoped to the touched location without mutating available totals', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/805',
        legacyResourceId: '805',
        title: 'Inventory Gloves',
        handle: 'inventory-gloves',
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
    store.replaceBaseVariantsForProduct('gid://shopify/Product/805', [
      {
        id: 'gid://shopify/ProductVariant/8051',
        productId: 'gid://shopify/Product/805',
        title: 'Default Title',
        sku: 'INV-GLOVES',
        barcode: null,
        price: '25.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 6,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8051',
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
          'mutation AdjustInventory($input: InventoryAdjustQuantitiesInput!) { inventoryAdjustQuantities(input: $input) { inventoryAdjustmentGroup { changes { name delta item { id } location { id } } } userErrors { field message } } }',
        variables: {
          input: {
            name: 'incoming',
            reason: 'correction',
            referenceDocumentUri: 'logistics://incoming/2026-04-18',
            changes: [
              {
                inventoryItemId: 'gid://shopify/InventoryItem/8051',
                locationId: 'gid://shopify/Location/2',
                ledgerDocumentUri: 'ledger://incoming/location-2',
                delta: 5,
              },
            ],
          },
        },
      });

    expect(mutationResponse.status).toBe(200);
    expect(mutationResponse.body.data.inventoryAdjustQuantities.userErrors).toEqual([]);

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($variantId: ID!, $inventoryItemId: ID!) { variant: productVariant(id: $variantId) { id inventoryQuantity inventoryItem { id inventoryLevels(first: 10) { nodes { location { id } quantities(names: ["available", "incoming", "on_hand"]) { name quantity } } } } } stock: inventoryItem(id: $inventoryItemId) { id inventoryLevels(first: 10) { nodes { location { id } quantities(names: ["available", "incoming", "on_hand"]) { name quantity } } } } }',
        variables: {
          variantId: 'gid://shopify/ProductVariant/8051',
          inventoryItemId: 'gid://shopify/InventoryItem/8051',
        },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        variant: {
          id: 'gid://shopify/ProductVariant/8051',
          inventoryQuantity: 6,
          inventoryItem: {
            id: 'gid://shopify/InventoryItem/8051',
            inventoryLevels: {
              nodes: [
                {
                  location: { id: 'gid://shopify/Location/1' },
                  quantities: [
                    { name: 'available', quantity: 6 },
                    { name: 'incoming', quantity: 0 },
                    { name: 'on_hand', quantity: 6 },
                  ],
                },
                {
                  location: { id: 'gid://shopify/Location/2' },
                  quantities: [
                    { name: 'available', quantity: 0 },
                    { name: 'incoming', quantity: 5 },
                    { name: 'on_hand', quantity: 0 },
                  ],
                },
              ],
            },
          },
        },
        stock: {
          id: 'gid://shopify/InventoryItem/8051',
          inventoryLevels: {
            nodes: [
              {
                location: { id: 'gid://shopify/Location/1' },
                quantities: [
                  { name: 'available', quantity: 6 },
                  { name: 'incoming', quantity: 0 },
                  { name: 'on_hand', quantity: 6 },
                ],
              },
              {
                location: { id: 'gid://shopify/Location/2' },
                quantities: [
                  { name: 'available', quantity: 0 },
                  { name: 'incoming', quantity: 5 },
                  { name: 'on_hand', quantity: 0 },
                ],
              },
            ],
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns Shopify-like user errors for invalid quantity names and missing ledgerDocumentUri', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/802',
        legacyResourceId: '802',
        title: 'Invalid Inventory Name Tee',
        handle: 'invalid-inventory-name-tee',
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
    store.replaceBaseVariantsForProduct('gid://shopify/Product/802', [
      {
        id: 'gid://shopify/ProductVariant/8021',
        productId: 'gid://shopify/Product/802',
        title: 'Default Title',
        sku: 'INV-INVALID',
        barcode: null,
        price: '25.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 6,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8021',
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
            name: 'on_hand',
            reason: 'correction',
            referenceDocumentUri: 'logistics://invalid/on-hand',
            changes: [
              {
                inventoryItemId: 'gid://shopify/InventoryItem/8021',
                locationId: 'gid://shopify/Location/1',
                delta: 1,
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
          userErrors: [
            {
              field: ['input', 'name'],
              message:
                'The specified quantity name is invalid. Valid values are: available, damaged, incoming, quality_control, reserved, safety_stock.',
            },
            {
              field: ['input', 'changes', '0', 'ledgerDocumentUri'],
              message: 'A ledger document URI is required except when adjusting available.',
            },
          ],
        },
      },
    });

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id totalInventory variants(first: 10) { nodes { id inventoryQuantity } } } }',
        variables: { id: 'gid://shopify/Product/802' },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        product: {
          id: 'gid://shopify/Product/802',
          totalInventory: 6,
          variants: {
            nodes: [{ id: 'gid://shopify/ProductVariant/8021', inventoryQuantity: 6 }],
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns Shopify-like invalid-variable errors for missing required inventory adjustment change fields', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/803',
        legacyResourceId: '803',
        title: 'Inventory Scarf',
        handle: 'inventory-scarf',
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
    store.replaceBaseVariantsForProduct('gid://shopify/Product/803', [
      {
        id: 'gid://shopify/ProductVariant/8031',
        productId: 'gid://shopify/Product/803',
        title: 'Default Title',
        sku: 'INV-SCARF',
        barcode: null,
        price: '20.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 6,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8031',
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
    const missingInventoryItemResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation AdjustInventory($input: InventoryAdjustQuantitiesInput!) { inventoryAdjustQuantities(input: $input) { inventoryAdjustmentGroup { changes { delta } } userErrors { field message } } }',
        variables: {
          input: {
            name: 'incoming',
            reason: 'correction',
            referenceDocumentUri: 'logistics://invalid/missing-item',
            changes: [
              {
                locationId: 'gid://shopify/Location/1',
                ledgerDocumentUri: 'ledger://missing-item',
                delta: 2,
              },
            ],
          },
        },
      });

    expect(missingInventoryItemResponse.status).toBe(200);
    expect(missingInventoryItemResponse.body).toEqual({
      errors: [
        {
          message:
            'Variable $input of type InventoryAdjustQuantitiesInput! was provided invalid value for changes.0.inventoryItemId (Expected value to not be null)',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: {
              name: 'incoming',
              reason: 'correction',
              referenceDocumentUri: 'logistics://invalid/missing-item',
              changes: [
                {
                  locationId: 'gid://shopify/Location/1',
                  ledgerDocumentUri: 'ledger://missing-item',
                  delta: 2,
                },
              ],
            },
            problems: [{ path: ['changes', 0, 'inventoryItemId'], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });

    const missingDeltaResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation AdjustInventory($input: InventoryAdjustQuantitiesInput!) { inventoryAdjustQuantities(input: $input) { inventoryAdjustmentGroup { changes { delta } } userErrors { field message } } }',
        variables: {
          input: {
            name: 'incoming',
            reason: 'correction',
            referenceDocumentUri: 'logistics://invalid/missing-delta',
            changes: [
              {
                inventoryItemId: 'gid://shopify/InventoryItem/8031',
                locationId: 'gid://shopify/Location/1',
                ledgerDocumentUri: 'ledger://missing-delta',
              },
            ],
          },
        },
      });

    expect(missingDeltaResponse.status).toBe(200);
    expect(missingDeltaResponse.body).toEqual({
      errors: [
        {
          message:
            'Variable $input of type InventoryAdjustQuantitiesInput! was provided invalid value for changes.0.delta (Expected value to not be null)',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: {
              name: 'incoming',
              reason: 'correction',
              referenceDocumentUri: 'logistics://invalid/missing-delta',
              changes: [
                {
                  inventoryItemId: 'gid://shopify/InventoryItem/8031',
                  locationId: 'gid://shopify/Location/1',
                  ledgerDocumentUri: 'ledger://missing-delta',
                },
              ],
            },
            problems: [{ path: ['changes', 0, 'delta'], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });

    const missingLocationResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation AdjustInventory($input: InventoryAdjustQuantitiesInput!) { inventoryAdjustQuantities(input: $input) { inventoryAdjustmentGroup { changes { delta } } userErrors { field message } } }',
        variables: {
          input: {
            name: 'incoming',
            reason: 'correction',
            referenceDocumentUri: 'logistics://invalid/missing-location',
            changes: [
              {
                inventoryItemId: 'gid://shopify/InventoryItem/8031',
                ledgerDocumentUri: 'ledger://missing-location',
                delta: 2,
              },
            ],
          },
        },
      });

    expect(missingLocationResponse.status).toBe(200);
    expect(missingLocationResponse.body).toEqual({
      errors: [
        {
          message:
            'Variable $input of type InventoryAdjustQuantitiesInput! was provided invalid value for changes.0.locationId (Expected value to not be null)',
          extensions: {
            code: 'INVALID_VARIABLE',
            value: {
              name: 'incoming',
              reason: 'correction',
              referenceDocumentUri: 'logistics://invalid/missing-location',
              changes: [
                {
                  inventoryItemId: 'gid://shopify/InventoryItem/8031',
                  ledgerDocumentUri: 'ledger://missing-location',
                  delta: 2,
                },
              ],
            },
            problems: [{ path: ['changes', 0, 'locationId'], explanation: 'Expected value to not be null' }],
          },
        },
      ],
    });

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id totalInventory variants(first: 10) { nodes { id inventoryQuantity } } } }',
        variables: { id: 'gid://shopify/Product/803' },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        product: {
          id: 'gid://shopify/Product/803',
          totalInventory: 6,
          variants: {
            nodes: [{ id: 'gid://shopify/ProductVariant/8031', inventoryQuantity: 6 }],
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns a user error for unknown inventory items without mutating downstream inventory reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/803',
        legacyResourceId: '803',
        title: 'Inventory Scarf',
        handle: 'inventory-scarf',
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
    store.replaceBaseVariantsForProduct('gid://shopify/Product/803', [
      {
        id: 'gid://shopify/ProductVariant/8031',
        productId: 'gid://shopify/Product/803',
        title: 'Default Title',
        sku: 'INV-SCARF',
        barcode: null,
        price: '20.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 6,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8031',
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
          userErrors: [
            {
              field: ['input', 'changes', '0', 'inventoryItemId'],
              message: 'The specified inventory item could not be found.',
            },
          ],
        },
      },
    });

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id totalInventory variants(first: 10) { nodes { id inventoryQuantity } } } }',
        variables: { id: 'gid://shopify/Product/803' },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        product: {
          id: 'gid://shopify/Product/803',
          totalInventory: 6,
          variants: {
            nodes: [{ id: 'gid://shopify/ProductVariant/8031', inventoryQuantity: 6 }],
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns the captured location-not-found userError when inventoryAdjustQuantities targets an unknown location id', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventoryAdjustQuantities should not hit upstream fetch');
    });

    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/804',
        legacyResourceId: '804',
        title: 'Inventory Unknown Location Hoodie',
        handle: 'inventory-unknown-location-hoodie',
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
    store.replaceBaseVariantsForProduct('gid://shopify/Product/804', [
      {
        id: 'gid://shopify/ProductVariant/8041',
        productId: 'gid://shopify/Product/804',
        title: 'Default Title',
        sku: 'INV-UNKNOWN-LOCATION',
        barcode: null,
        price: '20.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 6,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8041',
          tracked: true,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
          inventoryLevels: [
            {
              id: 'gid://shopify/InventoryLevel/8041?inventory_item_id=8041',
              cursor: 'cursor:gid://shopify/InventoryLevel/8041?inventory_item_id=8041',
              location: { id: 'gid://shopify/Location/1', name: 'Main warehouse' },
              quantities: [
                { name: 'available', quantity: 6, updatedAt: '2026-04-17T10:38:45Z' },
                { name: 'on_hand', quantity: 6, updatedAt: null },
                { name: 'incoming', quantity: 0, updatedAt: null },
              ],
            },
          ],
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
                inventoryItemId: 'gid://shopify/InventoryItem/8041',
                locationId: 'gid://shopify/Location/999999999999',
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
          userErrors: [
            {
              field: ['input', 'changes', '0', 'locationId'],
              message: 'The specified location could not be found.',
            },
          ],
        },
      },
    });

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query ($id: ID!) { product(id: $id) { id totalInventory variants(first: 10) { nodes { id inventoryQuantity inventoryItem { id inventoryLevels(first: 5) { nodes { location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } } } } } } } }',
        variables: { id: 'gid://shopify/Product/804' },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        product: {
          id: 'gid://shopify/Product/804',
          totalInventory: 6,
          variants: {
            nodes: [
              {
                id: 'gid://shopify/ProductVariant/8041',
                inventoryQuantity: 6,
                inventoryItem: {
                  id: 'gid://shopify/InventoryItem/8041',
                  inventoryLevels: {
                    nodes: [
                      {
                        location: { id: 'gid://shopify/Location/1', name: 'Main warehouse' },
                        quantities: [
                          { name: 'available', quantity: 6 },
                          { name: 'on_hand', quantity: 6 },
                          { name: 'incoming', quantity: 0 },
                        ],
                      },
                    ],
                  },
                },
              },
            ],
          },
        },
      },
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages inventoryActivate locally as a no-op when the inventory level is already active at the location', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventoryActivate should not hit upstream fetch');
    });

    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/807',
        legacyResourceId: '807',
        title: 'Inventory Activate Hoodie',
        handle: 'inventory-activate-hoodie',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'ACME',
        productType: 'APPAREL',
        tags: ['inventory'],
        totalInventory: 0,
        tracksInventory: false,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseVariantsForProduct('gid://shopify/Product/807', [
      {
        id: 'gid://shopify/ProductVariant/8071',
        productId: 'gid://shopify/Product/807',
        title: 'Default Title',
        sku: 'INV-ACTIVATE',
        barcode: null,
        price: '55.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 0,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8071',
          tracked: false,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
          inventoryLevels: [
            {
              id: 'gid://shopify/InventoryLevel/8071?inventory_item_id=8071',
              cursor: 'cursor:gid://shopify/InventoryLevel/8071?inventory_item_id=8071',
              location: { id: 'gid://shopify/Location/1', name: 'Main warehouse' },
              quantities: [
                { name: 'available', quantity: 0, updatedAt: '2026-04-17T10:38:45Z' },
                { name: 'on_hand', quantity: 0, updatedAt: null },
                { name: 'incoming', quantity: 0, updatedAt: null },
              ],
            },
          ],
        },
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();
    const mutationResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation ActivateInventory($inventoryItemId: ID!, $locationId: ID!) { inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId) { inventoryLevel { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity updatedAt } item { id tracked variant { id inventoryQuantity product { id totalInventory tracksInventory } } } } userErrors { field message } } }',
        variables: {
          inventoryItemId: 'gid://shopify/InventoryItem/8071',
          locationId: 'gid://shopify/Location/1',
        },
      });

    expect(mutationResponse.status).toBe(200);
    expect(mutationResponse.body).toEqual({
      data: {
        inventoryActivate: {
          inventoryLevel: {
            id: 'gid://shopify/InventoryLevel/8071?inventory_item_id=8071',
            location: { id: 'gid://shopify/Location/1', name: 'Main warehouse' },
            quantities: [
              { name: 'available', quantity: 0, updatedAt: '2026-04-17T10:38:45Z' },
              { name: 'on_hand', quantity: 0, updatedAt: null },
              { name: 'incoming', quantity: 0, updatedAt: null },
            ],
            item: {
              id: 'gid://shopify/InventoryItem/8071',
              tracked: false,
              variant: {
                id: 'gid://shopify/ProductVariant/8071',
                inventoryQuantity: 0,
                product: {
                  id: 'gid://shopify/Product/807',
                  totalInventory: 0,
                  tracksInventory: false,
                },
              },
            },
          },
          userErrors: [],
        },
      },
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns the captured location-not-found userErrors for inventoryActivate and inventoryBulkToggleActivation', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventory linkage mutations should not hit upstream fetch');
    });

    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/8072',
        legacyResourceId: '8072',
        title: 'Inventory Unknown Location Activate Hoodie',
        handle: 'inventory-unknown-location-activate-hoodie',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'ACME',
        productType: 'APPAREL',
        tags: ['inventory'],
        totalInventory: 4,
        tracksInventory: true,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseVariantsForProduct('gid://shopify/Product/8072', [
      {
        id: 'gid://shopify/ProductVariant/80721',
        productId: 'gid://shopify/Product/8072',
        title: 'Default Title',
        sku: 'INV-ACTIVATE-UNKNOWN-LOCATION',
        barcode: null,
        price: '55.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 4,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/80721',
          tracked: true,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
          inventoryLevels: [
            {
              id: 'gid://shopify/InventoryLevel/80721?inventory_item_id=80721',
              cursor: 'cursor:gid://shopify/InventoryLevel/80721?inventory_item_id=80721',
              location: { id: 'gid://shopify/Location/1', name: 'Main warehouse' },
              quantities: [
                { name: 'available', quantity: 4, updatedAt: '2026-04-17T10:38:45Z' },
                { name: 'on_hand', quantity: 4, updatedAt: null },
                { name: 'incoming', quantity: 0, updatedAt: null },
              ],
            },
          ],
        },
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();
    const activateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation ActivateInventory($inventoryItemId: ID!, $locationId: ID!) { inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId) { inventoryLevel { id } userErrors { field message } } }',
        variables: {
          inventoryItemId: 'gid://shopify/InventoryItem/80721',
          locationId: 'gid://shopify/Location/999999999999',
        },
      });

    expect(activateResponse.status).toBe(200);
    expect(activateResponse.body).toEqual({
      data: {
        inventoryActivate: {
          inventoryLevel: null,
          userErrors: [
            {
              field: ['locationId'],
              message: "The product couldn't be stocked because the location wasn't found.",
            },
          ],
        },
      },
    });

    const bulkResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation BulkToggleInventory($inventoryItemId: ID!, $inventoryItemUpdates: [InventoryBulkToggleActivationInput!]!) { inventoryBulkToggleActivation(inventoryItemId: $inventoryItemId, inventoryItemUpdates: $inventoryItemUpdates) { inventoryItem { id } inventoryLevels { id } userErrors { field message code } } }',
        variables: {
          inventoryItemId: 'gid://shopify/InventoryItem/80721',
          inventoryItemUpdates: [{ locationId: 'gid://shopify/Location/999999999999', activate: true }],
        },
      });

    expect(bulkResponse.status).toBe(200);
    expect(bulkResponse.body).toEqual({
      data: {
        inventoryBulkToggleActivation: {
          inventoryItem: null,
          inventoryLevels: null,
          userErrors: [
            {
              field: ['inventoryItemUpdates', '0', 'locationId'],
              message: "The quantity couldn't be updated because the location was not found.",
              code: 'LOCATION_NOT_FOUND',
            },
          ],
        },
      },
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns the captured single-location inventoryDeactivate blocker without mutating local inventory state', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventoryDeactivate should not hit upstream fetch');
    });

    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/808',
        legacyResourceId: '808',
        title: 'Inventory Deactivate Hoodie',
        handle: 'inventory-deactivate-hoodie',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'ACME',
        productType: 'APPAREL',
        tags: ['inventory'],
        totalInventory: 3,
        tracksInventory: false,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseVariantsForProduct('gid://shopify/Product/808', [
      {
        id: 'gid://shopify/ProductVariant/8081',
        productId: 'gid://shopify/Product/808',
        title: 'Default Title',
        sku: 'INV-DEACTIVATE',
        barcode: null,
        price: '55.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 3,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8081',
          tracked: false,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
          inventoryLevels: [
            {
              id: 'gid://shopify/InventoryLevel/8081?inventory_item_id=8081',
              cursor: 'cursor:gid://shopify/InventoryLevel/8081?inventory_item_id=8081',
              location: { id: 'gid://shopify/Location/1', name: '103 ossington' },
              quantities: [
                { name: 'available', quantity: 3, updatedAt: '2026-04-17T10:38:45Z' },
                { name: 'on_hand', quantity: 3, updatedAt: null },
                { name: 'incoming', quantity: 0, updatedAt: null },
              ],
            },
          ],
        },
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();
    const mutationResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeactivateInventory($inventoryLevelId: ID!) { inventoryDeactivate(inventoryLevelId: $inventoryLevelId) { userErrors { field message } } }',
        variables: {
          inventoryLevelId: 'gid://shopify/InventoryLevel/8081?inventory_item_id=8081',
        },
      });

    expect(mutationResponse.status).toBe(200);
    expect(mutationResponse.body).toEqual({
      data: {
        inventoryDeactivate: {
          userErrors: [
            {
              field: null,
              message:
                "The product couldn't be unstocked from 103 ossington because products need to be stocked at a minimum of 1 location.",
            },
          ],
        },
      },
    });

    const queryResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query InspectDeactivate($inventoryItemId: ID!) { inventoryItem(id: $inventoryItemId) { id tracked inventoryLevels(first: 5) { nodes { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } } } } }',
        variables: {
          inventoryItemId: 'gid://shopify/InventoryItem/8081',
        },
      });

    expect(queryResponse.status).toBe(200);
    expect(queryResponse.body).toEqual({
      data: {
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8081',
          tracked: false,
          inventoryLevels: {
            nodes: [
              {
                id: 'gid://shopify/InventoryLevel/8081?inventory_item_id=8081',
                location: { id: 'gid://shopify/Location/1', name: '103 ossington' },
                quantities: [
                  { name: 'available', quantity: 3 },
                  { name: 'on_hand', quantity: 3 },
                  { name: 'incoming', quantity: 0 },
                ],
              },
            ],
          },
        },
      },
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages inventoryBulkToggleActivation locally for activate:true no-op success and single-location deactivate failure', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventoryBulkToggleActivation should not hit upstream fetch');
    });

    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/809',
        legacyResourceId: '809',
        title: 'Inventory Bulk Toggle Hoodie',
        handle: 'inventory-bulk-toggle-hoodie',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'ACME',
        productType: 'APPAREL',
        tags: ['inventory'],
        totalInventory: 5,
        tracksInventory: false,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseVariantsForProduct('gid://shopify/Product/809', [
      {
        id: 'gid://shopify/ProductVariant/8091',
        productId: 'gid://shopify/Product/809',
        title: 'Default Title',
        sku: 'INV-BULK-TOGGLE',
        barcode: null,
        price: '55.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 5,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8091',
          tracked: false,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
          inventoryLevels: [
            {
              id: 'gid://shopify/InventoryLevel/8091?inventory_item_id=8091',
              cursor: 'cursor:gid://shopify/InventoryLevel/8091?inventory_item_id=8091',
              location: { id: 'gid://shopify/Location/1', name: '103 ossington' },
              quantities: [
                { name: 'available', quantity: 5, updatedAt: '2026-04-17T10:38:45Z' },
                { name: 'on_hand', quantity: 5, updatedAt: null },
                { name: 'incoming', quantity: 0, updatedAt: null },
              ],
            },
          ],
        },
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();
    const activateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation BulkToggleInventory($inventoryItemId: ID!, $inventoryItemUpdates: [InventoryBulkToggleActivationInput!]!) { inventoryBulkToggleActivation(inventoryItemId: $inventoryItemId, inventoryItemUpdates: $inventoryItemUpdates) { inventoryItem { id tracked inventoryLevels(first: 5) { nodes { id location { id } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } } } } inventoryLevels { id location { id } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } item { id tracked } } userErrors { field message code } } }',
        variables: {
          inventoryItemId: 'gid://shopify/InventoryItem/8091',
          inventoryItemUpdates: [{ locationId: 'gid://shopify/Location/1', activate: true }],
        },
      });

    expect(activateResponse.status).toBe(200);
    expect(activateResponse.body).toEqual({
      data: {
        inventoryBulkToggleActivation: {
          inventoryItem: {
            id: 'gid://shopify/InventoryItem/8091',
            tracked: false,
            inventoryLevels: {
              nodes: [
                {
                  id: 'gid://shopify/InventoryLevel/8091?inventory_item_id=8091',
                  location: { id: 'gid://shopify/Location/1' },
                  quantities: [
                    { name: 'available', quantity: 5 },
                    { name: 'on_hand', quantity: 5 },
                    { name: 'incoming', quantity: 0 },
                  ],
                },
              ],
            },
          },
          inventoryLevels: [
            {
              id: 'gid://shopify/InventoryLevel/8091?inventory_item_id=8091',
              location: { id: 'gid://shopify/Location/1' },
              quantities: [
                { name: 'available', quantity: 5 },
                { name: 'on_hand', quantity: 5 },
                { name: 'incoming', quantity: 0 },
              ],
              item: {
                id: 'gid://shopify/InventoryItem/8091',
                tracked: false,
              },
            },
          ],
          userErrors: [],
        },
      },
    });

    const deactivateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation BulkToggleInventory($inventoryItemId: ID!, $inventoryItemUpdates: [InventoryBulkToggleActivationInput!]!) { inventoryBulkToggleActivation(inventoryItemId: $inventoryItemId, inventoryItemUpdates: $inventoryItemUpdates) { inventoryItem { id } inventoryLevels { id } userErrors { field message code } } }',
        variables: {
          inventoryItemId: 'gid://shopify/InventoryItem/8091',
          inventoryItemUpdates: [{ locationId: 'gid://shopify/Location/1', activate: false }],
        },
      });

    expect(deactivateResponse.status).toBe(200);
    expect(deactivateResponse.body).toEqual({
      data: {
        inventoryBulkToggleActivation: {
          inventoryItem: null,
          inventoryLevels: null,
          userErrors: [
            {
              field: ['inventoryItemUpdates', '0', 'locationId'],
              message:
                "The variant couldn't be unstocked from 103 ossington because products need to be stocked at a minimum of 1 location.",
              code: 'CANNOT_DEACTIVATE_FROM_ONLY_LOCATION',
            },
          ],
        },
      },
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('activates a known second location locally and allows inventoryDeactivate once another level exists', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventory linkage success path should not hit upstream fetch');
    });

    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/810',
        legacyResourceId: '810',
        title: 'Inventory Activate Hoodie',
        handle: 'inventory-activate-hoodie',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'ACME',
        productType: 'APPAREL',
        tags: ['inventory'],
        totalInventory: 0,
        tracksInventory: false,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
      {
        id: 'gid://shopify/Product/811',
        legacyResourceId: '811',
        title: 'Known Second Location Hoodie',
        handle: 'known-second-location-hoodie',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'ACME',
        productType: 'APPAREL',
        tags: ['inventory'],
        totalInventory: 0,
        tracksInventory: false,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseVariantsForProduct('gid://shopify/Product/810', [
      {
        id: 'gid://shopify/ProductVariant/8101',
        productId: 'gid://shopify/Product/810',
        title: 'Default Title',
        sku: 'INV-ACTIVATE',
        barcode: null,
        price: '55.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 0,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8101',
          tracked: false,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
          inventoryLevels: [
            {
              id: 'gid://shopify/InventoryLevel/8101?inventory_item_id=8101',
              cursor: 'cursor:gid://shopify/InventoryLevel/8101?inventory_item_id=8101',
              location: { id: 'gid://shopify/Location/68509171945', name: '103 ossington' },
              quantities: [
                { name: 'available', quantity: 0, updatedAt: '2026-04-17T12:44:00Z' },
                { name: 'on_hand', quantity: 0, updatedAt: null },
                { name: 'incoming', quantity: 0, updatedAt: null },
              ],
            },
          ],
        },
      },
    ]);
    store.replaceBaseVariantsForProduct('gid://shopify/Product/811', [
      {
        id: 'gid://shopify/ProductVariant/8111',
        productId: 'gid://shopify/Product/811',
        title: 'Default Title',
        sku: 'INV-KNOWN-LOC',
        barcode: null,
        price: '55.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 0,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8111',
          tracked: false,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
          inventoryLevels: [
            {
              id: 'gid://shopify/InventoryLevel/8111?inventory_item_id=8111',
              cursor: 'cursor:gid://shopify/InventoryLevel/8111?inventory_item_id=8111',
              location: { id: 'gid://shopify/Location/89026920681', name: 'Hermes Conformance Annex' },
              quantities: [
                { name: 'available', quantity: 0, updatedAt: '2026-04-17T12:44:01Z' },
                { name: 'on_hand', quantity: 0, updatedAt: null },
                { name: 'incoming', quantity: 0, updatedAt: null },
              ],
            },
          ],
        },
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();
    const activateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation ActivateInventory($inventoryItemId: ID!, $locationId: ID!, $available: Int) { inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId, available: $available) { inventoryLevel { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } item { id tracked variant { id inventoryQuantity product { id totalInventory tracksInventory } } } } userErrors { field message } } }',
        variables: {
          inventoryItemId: 'gid://shopify/InventoryItem/8101',
          locationId: 'gid://shopify/Location/89026920681',
          available: 9,
        },
      });

    expect(activateResponse.status).toBe(200);
    expect(activateResponse.body).toEqual({
      data: {
        inventoryActivate: {
          inventoryLevel: {
            id: expect.any(String),
            location: { id: 'gid://shopify/Location/89026920681', name: 'Hermes Conformance Annex' },
            quantities: [
              { name: 'available', quantity: 0 },
              { name: 'on_hand', quantity: 0 },
              { name: 'incoming', quantity: 0 },
            ],
            item: {
              id: 'gid://shopify/InventoryItem/8101',
              tracked: false,
              variant: {
                id: 'gid://shopify/ProductVariant/8101',
                inventoryQuantity: 0,
                product: {
                  id: 'gid://shopify/Product/810',
                  totalInventory: 0,
                  tracksInventory: false,
                },
              },
            },
          },
          userErrors: [],
        },
      },
    });

    const postActivateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query InspectActivatedInventory($inventoryItemId: ID!) { inventoryItem(id: $inventoryItemId) { id tracked inventoryLevels(first: 10) { nodes { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } } } } }',
        variables: {
          inventoryItemId: 'gid://shopify/InventoryItem/8101',
        },
      });

    expect(postActivateResponse.status).toBe(200);
    expect(postActivateResponse.body).toEqual({
      data: {
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8101',
          tracked: false,
          inventoryLevels: {
            nodes: [
              {
                id: 'gid://shopify/InventoryLevel/8101?inventory_item_id=8101',
                location: { id: 'gid://shopify/Location/68509171945', name: '103 ossington' },
                quantities: [
                  { name: 'available', quantity: 0 },
                  { name: 'on_hand', quantity: 0 },
                  { name: 'incoming', quantity: 0 },
                ],
              },
              {
                id: expect.any(String),
                location: { id: 'gid://shopify/Location/89026920681', name: 'Hermes Conformance Annex' },
                quantities: [
                  { name: 'available', quantity: 0 },
                  { name: 'on_hand', quantity: 0 },
                  { name: 'incoming', quantity: 0 },
                ],
              },
            ],
          },
        },
      },
    });

    const activatedLevelId = postActivateResponse.body.data.inventoryItem.inventoryLevels.nodes[1].id;
    expect(activatedLevelId).toEqual(expect.any(String));
    const deactivateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation DeactivateInventory($inventoryLevelId: ID!) { inventoryDeactivate(inventoryLevelId: $inventoryLevelId) { userErrors { field message } } }',
        variables: {
          inventoryLevelId: 'gid://shopify/InventoryLevel/8101?inventory_item_id=8101',
        },
      });

    expect(deactivateResponse.status).toBe(200);
    expect(deactivateResponse.body).toEqual({
      data: {
        inventoryDeactivate: {
          userErrors: [],
        },
      },
    });

    const postDeactivateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query InspectDeactivate($inventoryItemId: ID!, $variantId: ID!) { inventoryItem(id: $inventoryItemId) { id tracked inventoryLevels(first: 10) { nodes { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } } } variant { id inventoryQuantity product { id totalInventory tracksInventory } } } productVariant(id: $variantId) { id inventoryQuantity inventoryItem { id tracked inventoryLevels(first: 10) { nodes { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } } } } } }',
        variables: {
          inventoryItemId: 'gid://shopify/InventoryItem/8101',
          variantId: 'gid://shopify/ProductVariant/8101',
        },
      });

    expect(postDeactivateResponse.status).toBe(200);
    expect(postDeactivateResponse.body).toEqual({
      data: {
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8101',
          tracked: false,
          inventoryLevels: {
            nodes: [
              {
                id: activatedLevelId,
                location: { id: 'gid://shopify/Location/89026920681', name: 'Hermes Conformance Annex' },
                quantities: [
                  { name: 'available', quantity: 0 },
                  { name: 'on_hand', quantity: 0 },
                  { name: 'incoming', quantity: 0 },
                ],
              },
            ],
          },
          variant: {
            id: 'gid://shopify/ProductVariant/8101',
            inventoryQuantity: 0,
            product: {
              id: 'gid://shopify/Product/810',
              totalInventory: 0,
              tracksInventory: false,
            },
          },
        },
        productVariant: {
          id: 'gid://shopify/ProductVariant/8101',
          inventoryQuantity: 0,
          inventoryItem: {
            id: 'gid://shopify/InventoryItem/8101',
            tracked: false,
            inventoryLevels: {
              nodes: [
                {
                  id: activatedLevelId,
                  location: { id: 'gid://shopify/Location/89026920681', name: 'Hermes Conformance Annex' },
                  quantities: [
                    { name: 'available', quantity: 0 },
                    { name: 'on_hand', quantity: 0 },
                    { name: 'incoming', quantity: 0 },
                  ],
                },
              ],
            },
          },
        },
      },
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages inventoryBulkToggleActivation locally for multi-location activate:true and deactivate:false success', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('inventoryBulkToggleActivation success path should not hit upstream fetch');
    });

    store.upsertBaseProducts([
      {
        id: 'gid://shopify/Product/812',
        legacyResourceId: '812',
        title: 'Inventory Bulk Multi Hoodie',
        handle: 'inventory-bulk-multi-hoodie',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'ACME',
        productType: 'APPAREL',
        tags: ['inventory'],
        totalInventory: 0,
        tracksInventory: false,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
      {
        id: 'gid://shopify/Product/813',
        legacyResourceId: '813',
        title: 'Known Second Bulk Location Hoodie',
        handle: 'known-second-bulk-location-hoodie',
        status: 'ACTIVE',
        publicationIds: [],
        createdAt: '2024-01-01T00:00:00.000Z',
        updatedAt: '2024-01-02T00:00:00.000Z',
        vendor: 'ACME',
        productType: 'APPAREL',
        tags: ['inventory'],
        totalInventory: 0,
        tracksInventory: false,
        descriptionHtml: null,
        onlineStorePreviewUrl: null,
        templateSuffix: null,
        seo: { title: null, description: null },
        category: null,
      },
    ]);
    store.replaceBaseVariantsForProduct('gid://shopify/Product/812', [
      {
        id: 'gid://shopify/ProductVariant/8121',
        productId: 'gid://shopify/Product/812',
        title: 'Default Title',
        sku: 'INV-BULK-MULTI',
        barcode: null,
        price: '55.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 0,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8121',
          tracked: false,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
          inventoryLevels: [
            {
              id: 'gid://shopify/InventoryLevel/8121?inventory_item_id=8121',
              cursor: 'cursor:gid://shopify/InventoryLevel/8121?inventory_item_id=8121',
              location: { id: 'gid://shopify/Location/68509171945', name: '103 ossington' },
              quantities: [
                { name: 'available', quantity: 0, updatedAt: '2026-04-17T12:45:48Z' },
                { name: 'on_hand', quantity: 0, updatedAt: null },
                { name: 'incoming', quantity: 0, updatedAt: null },
              ],
            },
          ],
        },
      },
    ]);
    store.replaceBaseVariantsForProduct('gid://shopify/Product/813', [
      {
        id: 'gid://shopify/ProductVariant/8131',
        productId: 'gid://shopify/Product/813',
        title: 'Default Title',
        sku: 'INV-BULK-KNOWN-LOC',
        barcode: null,
        price: '55.00',
        compareAtPrice: null,
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryQuantity: 0,
        selectedOptions: [],
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8131',
          tracked: false,
          requiresShipping: true,
          measurement: null,
          countryCodeOfOrigin: null,
          provinceCodeOfOrigin: null,
          harmonizedSystemCode: null,
          inventoryLevels: [
            {
              id: 'gid://shopify/InventoryLevel/8131?inventory_item_id=8131',
              cursor: 'cursor:gid://shopify/InventoryLevel/8131?inventory_item_id=8131',
              location: { id: 'gid://shopify/Location/89026920681', name: 'Hermes Conformance Annex' },
              quantities: [
                { name: 'available', quantity: 0, updatedAt: '2026-04-17T12:45:48Z' },
                { name: 'on_hand', quantity: 0, updatedAt: null },
                { name: 'incoming', quantity: 0, updatedAt: null },
              ],
            },
          ],
        },
      },
    ]);

    const app = createApp({ ...config, readMode: 'snapshot' }).callback();
    const activateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation BulkToggleInventory($inventoryItemId: ID!, $inventoryItemUpdates: [InventoryBulkToggleActivationInput!]!) { inventoryBulkToggleActivation(inventoryItemId: $inventoryItemId, inventoryItemUpdates: $inventoryItemUpdates) { inventoryItem { id tracked inventoryLevels(first: 5) { nodes { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } } } } inventoryLevels { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } item { id tracked } } userErrors { field message code } } }',
        variables: {
          inventoryItemId: 'gid://shopify/InventoryItem/8121',
          inventoryItemUpdates: [{ locationId: 'gid://shopify/Location/89026920681', activate: true }],
        },
      });

    expect(activateResponse.status).toBe(200);
    expect(activateResponse.body).toEqual({
      data: {
        inventoryBulkToggleActivation: {
          inventoryItem: {
            id: 'gid://shopify/InventoryItem/8121',
            tracked: false,
            inventoryLevels: {
              nodes: [
                {
                  id: 'gid://shopify/InventoryLevel/8121?inventory_item_id=8121',
                  location: { id: 'gid://shopify/Location/68509171945', name: '103 ossington' },
                  quantities: [
                    { name: 'available', quantity: 0 },
                    { name: 'on_hand', quantity: 0 },
                    { name: 'incoming', quantity: 0 },
                  ],
                },
                {
                  id: expect.any(String),
                  location: { id: 'gid://shopify/Location/89026920681', name: 'Hermes Conformance Annex' },
                  quantities: [
                    { name: 'available', quantity: 0 },
                    { name: 'on_hand', quantity: 0 },
                    { name: 'incoming', quantity: 0 },
                  ],
                },
              ],
            },
          },
          inventoryLevels: [
            {
              id: expect.any(String),
              location: { id: 'gid://shopify/Location/89026920681', name: 'Hermes Conformance Annex' },
              quantities: [
                { name: 'available', quantity: 0 },
                { name: 'on_hand', quantity: 0 },
                { name: 'incoming', quantity: 0 },
              ],
              item: {
                id: 'gid://shopify/InventoryItem/8121',
                tracked: false,
              },
            },
          ],
          userErrors: [],
        },
      },
    });

    const deactivateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation BulkToggleInventory($inventoryItemId: ID!, $inventoryItemUpdates: [InventoryBulkToggleActivationInput!]!) { inventoryBulkToggleActivation(inventoryItemId: $inventoryItemId, inventoryItemUpdates: $inventoryItemUpdates) { inventoryItem { id tracked inventoryLevels(first: 5) { nodes { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } } } } inventoryLevels { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } item { id tracked } } userErrors { field message code } } }',
        variables: {
          inventoryItemId: 'gid://shopify/InventoryItem/8121',
          inventoryItemUpdates: [{ locationId: 'gid://shopify/Location/89026920681', activate: false }],
        },
      });

    expect(deactivateResponse.status).toBe(200);
    expect(deactivateResponse.body).toEqual({
      data: {
        inventoryBulkToggleActivation: {
          inventoryItem: {
            id: 'gid://shopify/InventoryItem/8121',
            tracked: false,
            inventoryLevels: {
              nodes: [
                {
                  id: 'gid://shopify/InventoryLevel/8121?inventory_item_id=8121',
                  location: { id: 'gid://shopify/Location/68509171945', name: '103 ossington' },
                  quantities: [
                    { name: 'available', quantity: 0 },
                    { name: 'on_hand', quantity: 0 },
                    { name: 'incoming', quantity: 0 },
                  ],
                },
              ],
            },
          },
          inventoryLevels: [],
          userErrors: [],
        },
      },
    });

    const postDeactivateResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query InspectBulkDeactivate($inventoryItemId: ID!) { inventoryItem(id: $inventoryItemId) { id tracked inventoryLevels(first: 10) { nodes { id location { id name } quantities(names: ["available", "on_hand", "incoming"]) { name quantity } } } } }',
        variables: {
          inventoryItemId: 'gid://shopify/InventoryItem/8121',
        },
      });

    expect(postDeactivateResponse.status).toBe(200);
    expect(postDeactivateResponse.body).toEqual({
      data: {
        inventoryItem: {
          id: 'gid://shopify/InventoryItem/8121',
          tracked: false,
          inventoryLevels: {
            nodes: [
              {
                id: 'gid://shopify/InventoryLevel/8121?inventory_item_id=8121',
                location: { id: 'gid://shopify/Location/68509171945', name: '103 ossington' },
                quantities: [
                  { name: 'available', quantity: 0 },
                  { name: 'on_hand', quantity: 0 },
                  { name: 'incoming', quantity: 0 },
                ],
              },
            ],
          },
        },
      },
    });

    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
