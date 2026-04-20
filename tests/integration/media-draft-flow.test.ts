import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

describe('media draft flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages fileCreate locally without attaching product media or proxying upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('fileCreate should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const productResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Media file owner" }) { product { id } userErrors { field message } } }',
    });

    const productId = productResponse.body.data.productCreate.product.id as string;
    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileCreate($files: [FileCreateInput!]!) { fileCreate(files: $files) { files { id fileStatus alt createdAt ... on MediaImage { image { url width height } } } userErrors { field message code } } }',
        variables: {
          files: [
            {
              alt: 'Lookbook hero image',
              contentType: 'IMAGE',
              filename: 'lookbook-hero.jpg',
              originalSource: 'https://cdn.example.com/lookbook-hero.jpg',
            },
          ],
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.fileCreate.userErrors).toEqual([]);
    expect(createResponse.body.data.fileCreate.files).toEqual([
      {
        id: expect.stringMatching(/^gid:\/\/shopify\/MediaImage\//),
        fileStatus: 'READY',
        alt: 'Lookbook hero image',
        createdAt: expect.any(String),
        image: {
          url: 'https://cdn.example.com/lookbook-hero.jpg',
          width: null,
          height: null,
        },
      },
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();

    const productMediaResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'query ProductMedia($id: ID!) { product(id: $id) { id media(first: 10) { nodes { id alt mediaContentType } pageInfo { hasNextPage hasPreviousPage } } } }',
        variables: { id: productId },
      });

    expect(productMediaResponse.status).toBe(200);
    expect(productMediaResponse.body.data.product.media).toEqual({
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
      },
    });

    const stateResponse = await request(app).get('/__meta/state');
    expect(Object.values(stateResponse.body.stagedState.files)).toEqual([
      expect.objectContaining({
        alt: 'Lookbook hero image',
        contentType: 'IMAGE',
        filename: 'lookbook-hero.jpg',
        originalSource: 'https://cdn.example.com/lookbook-hero.jpg',
      }),
    ]);

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.at(-1)).toMatchObject({
      operationName: 'FileCreate',
      query: expect.stringContaining('mutation FileCreate'),
      variables: {
        files: [
          {
            alt: 'Lookbook hero image',
            contentType: 'IMAGE',
            filename: 'lookbook-hero.jpg',
            originalSource: 'https://cdn.example.com/lookbook-hero.jpg',
          },
        ],
      },
      status: 'staged',
      interpreted: {
        primaryRootField: 'fileCreate',
        capability: {
          domain: 'media',
          execution: 'stage-locally',
        },
      },
    });
  });

  it('returns Shopify-like fileCreate user errors without staging invalid file inputs', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid fileCreate should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query:
        'mutation { fileCreate(files: [{ contentType: IMAGE, originalSource: "not-a-valid-url", alt: "Bad" }]) { files { id } userErrors { field message code } } }',
    });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.fileCreate).toEqual({
      files: [],
      userErrors: [
        {
          field: ['files', '0', 'originalSource'],
          message: 'Image URL is invalid',
          code: 'INVALID',
        },
      ],
    });
    expect(store.getState().stagedState.files).toEqual({});
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
