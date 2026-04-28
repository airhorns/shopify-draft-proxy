import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';

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
        fileStatus: 'UPLOADED',
        alt: 'Lookbook hero image',
        createdAt: expect.any(String),
        image: null,
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

  it('serves local files and empty file saved searches in snapshot mode without proxying upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('snapshot file reads should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileCreate($files: [FileCreateInput!]!) { fileCreate(files: $files) { files { id __typename alt fileStatus filename ... on GenericFile { id } ... on MediaImage { image { url } } } userErrors { field message code } } }',
        variables: {
          files: [
            {
              alt: 'Spec sheet',
              contentType: 'FILE',
              filename: 'spec-sheet.pdf',
              originalSource: 'https://cdn.example.com/spec-sheet.pdf',
            },
            {
              alt: 'Gallery image',
              contentType: 'IMAGE',
              filename: 'gallery.jpg',
              originalSource: 'https://cdn.example.com/gallery.jpg',
            },
          ],
        },
      });

    const firstFileId = createResponse.body.data.fileCreate.files[0].id as string;
    const secondFileId = createResponse.body.data.fileCreate.files[1].id as string;

    const firstPageResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'query FilesCatalog($first: Int) { files(first: $first) { edges { cursor node { id __typename alt fileStatus filename } } nodes { id alt } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } fileSavedSearches(first: 5) { nodes { id name } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: { first: 1 },
      });

    expect(firstPageResponse.status).toBe(200);
    expect(firstPageResponse.body.data.files).toEqual({
      edges: [
        {
          cursor: `cursor:${firstFileId}`,
          node: {
            id: firstFileId,
            __typename: 'GenericFile',
            alt: 'Spec sheet',
            fileStatus: 'UPLOADED',
            filename: 'spec-sheet.pdf',
          },
        },
      ],
      nodes: [
        {
          id: firstFileId,
          alt: 'Spec sheet',
        },
      ],
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: false,
        startCursor: `cursor:${firstFileId}`,
        endCursor: `cursor:${firstFileId}`,
      },
    });
    expect(firstPageResponse.body.data.fileSavedSearches).toEqual({
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    });

    const secondPageResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'query FilesAfter($after: String) { files(first: 10, after: $after) { nodes { id __typename alt ... on MediaImage { image { url width height } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: { after: `cursor:${firstFileId}` },
      });

    expect(secondPageResponse.body.data.files).toEqual({
      nodes: [
        {
          id: secondFileId,
          __typename: 'MediaImage',
          alt: 'Gallery image',
          image: null,
        },
      ],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: true,
        startCursor: `cursor:${secondFileId}`,
        endCursor: `cursor:${secondFileId}`,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns inert stagedUploadsCreate metadata locally without creating upload side effects', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('stagedUploadsCreate should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const uploadResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation StagedUploadsCreate($input: [StagedUploadInput!]!) { stagedUploadsCreate(input: $input) { stagedTargets { url resourceUrl parameters { name value } } userErrors { field message code } } }',
        variables: {
          input: [
            {
              filename: 'safe-upload-image.png',
              mimeType: 'image/png',
              resource: 'IMAGE',
              httpMethod: 'POST',
            },
            {
              filename: 'safe-upload-file.txt',
              mimeType: 'text/plain',
              resource: 'FILE',
              httpMethod: 'POST',
            },
            {
              filename: 'safe-upload-video.mp4',
              mimeType: 'video/mp4',
              resource: 'VIDEO',
              httpMethod: 'POST',
              fileSize: '4096',
            },
            {
              filename: 'safe-upload-model.glb',
              mimeType: 'model/gltf-binary',
              resource: 'MODEL_3D',
              httpMethod: 'POST',
              fileSize: '4096',
            },
          ],
        },
      });

    expect(uploadResponse.status).toBe(200);
    expect(uploadResponse.body.data.stagedUploadsCreate.userErrors).toEqual([]);
    const stagedTargets = uploadResponse.body.data.stagedUploadsCreate.stagedTargets as Array<{
      url: string;
      resourceUrl: string;
      parameters: Array<{ name: string; value: string }>;
    }>;
    expect(stagedTargets).toHaveLength(4);
    expect(stagedTargets.map((target) => target.parameters.map((parameter) => parameter.name))).toEqual([
      [
        'Content-Type',
        'success_action_status',
        'acl',
        'key',
        'x-goog-date',
        'x-goog-credential',
        'x-goog-algorithm',
        'x-goog-signature',
        'policy',
      ],
      [
        'Content-Type',
        'success_action_status',
        'acl',
        'key',
        'x-goog-date',
        'x-goog-credential',
        'x-goog-algorithm',
        'x-goog-signature',
        'policy',
      ],
      ['GoogleAccessId', 'key', 'policy', 'signature'],
      ['GoogleAccessId', 'key', 'policy', 'signature'],
    ]);
    expect(stagedTargets).toEqual([
      expect.objectContaining({
        url: expect.stringMatching(/^https:\/\/shopify-draft-proxy\.local\/staged-uploads\//),
        resourceUrl: expect.stringContaining('/safe-upload-image.png'),
        parameters: expect.arrayContaining([
          { name: 'Content-Type', value: 'image/png' },
          { name: 'success_action_status', value: '201' },
          { name: 'acl', value: 'private' },
          { name: 'x-goog-algorithm', value: 'GOOG4-RSA-SHA256' },
        ]),
      }),
      expect.objectContaining({
        url: expect.stringMatching(/^https:\/\/shopify-draft-proxy\.local\/staged-uploads\//),
        resourceUrl: expect.stringContaining('/safe-upload-file.txt'),
        parameters: expect.arrayContaining([
          { name: 'Content-Type', value: 'text/plain' },
          { name: 'success_action_status', value: '201' },
          { name: 'acl', value: 'private' },
          { name: 'x-goog-algorithm', value: 'GOOG4-RSA-SHA256' },
        ]),
      }),
      expect.objectContaining({
        url: expect.stringMatching(/^https:\/\/shopify-draft-proxy\.local\/staged-uploads\//),
        resourceUrl: expect.stringContaining('/safe-upload-video.mp4'),
      }),
      expect.objectContaining({
        url: expect.stringMatching(/^https:\/\/shopify-draft-proxy\.local\/staged-uploads\//),
        resourceUrl: expect.stringContaining('/safe-upload-model.glb'),
      }),
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getState().stagedState.files).toEqual({});

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.at(-1)).toMatchObject({
      operationName: 'StagedUploadsCreate',
      status: 'staged',
      interpreted: {
        primaryRootField: 'stagedUploadsCreate',
        capability: {
          domain: 'media',
          execution: 'stage-locally',
        },
      },
      notes: 'Staged locally in the in-memory media draft store.',
    });
  });

  it('stages fileAcknowledgeUpdateFailed locally and preserves downstream file reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('fileAcknowledgeUpdateFailed should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileCreate($files: [FileCreateInput!]!) { fileCreate(files: $files) { files { id alt filename fileStatus } userErrors { field message code } } }',
        variables: {
          files: [
            {
              alt: 'Failure acknowledgement source',
              contentType: 'IMAGE',
              filename: 'ack-source.jpg',
              originalSource: 'https://cdn.example.com/ack-source.jpg',
            },
          ],
        },
      });
    const fileId = createResponse.body.data.fileCreate.files[0].id as string;

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileUpdate($files: [FileUpdateInput!]!) { fileUpdate(files: $files) { files { id alt fileStatus ... on MediaImage { image { url } } } userErrors { field message code } } }',
        variables: {
          files: [
            {
              id: fileId,
              alt: 'Failure acknowledgement ready state',
              originalSource: 'https://cdn.example.com/ack-ready.jpg',
            },
          ],
        },
      });
    expect(updateResponse.body.data.fileUpdate.userErrors).toEqual([]);
    expect(updateResponse.body.data.fileUpdate.files[0]).toMatchObject({
      id: fileId,
      alt: 'Failure acknowledgement ready state',
      fileStatus: 'READY',
      image: {
        url: 'https://cdn.example.com/ack-ready.jpg',
      },
    });

    const acknowledgeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileAcknowledgeUpdateFailed($fileIds: [ID!]!) { fileAcknowledgeUpdateFailed(fileIds: $fileIds) { files { id alt fileStatus ... on MediaImage { image { url } } } userErrors { field message code } } }',
        variables: { fileIds: [fileId] },
      });

    expect(acknowledgeResponse.status).toBe(200);
    expect(acknowledgeResponse.body.data.fileAcknowledgeUpdateFailed).toEqual({
      files: [
        {
          id: fileId,
          alt: 'Failure acknowledgement ready state',
          fileStatus: 'READY',
          image: {
            url: 'https://cdn.example.com/ack-ready.jpg',
          },
        },
      ],
      userErrors: [],
    });
    expect(fetchSpy).not.toHaveBeenCalled();

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.files[fileId]).toMatchObject({
      id: fileId,
      fileStatus: 'READY',
      updateFailureAcknowledgedAt: expect.any(String),
    });

    const filesResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query:
        'query FilesAfterAck { files(first: 10) { nodes { id alt fileStatus ... on MediaImage { image { url } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
    });
    expect(filesResponse.body.data.files.nodes).toEqual([
      {
        id: fileId,
        alt: 'Failure acknowledgement ready state',
        fileStatus: 'READY',
        image: {
          url: 'https://cdn.example.com/ack-ready.jpg',
        },
      },
    ]);

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.at(-1)).toMatchObject({
      operationName: 'FileAcknowledgeUpdateFailed',
      query: expect.stringContaining('mutation FileAcknowledgeUpdateFailed'),
      variables: { fileIds: [fileId] },
      status: 'staged',
      interpreted: {
        primaryRootField: 'fileAcknowledgeUpdateFailed',
        capability: {
          domain: 'media',
          execution: 'stage-locally',
        },
      },
      notes: 'Staged locally in the in-memory media draft store.',
    });
  });

  it('returns fileAcknowledgeUpdateFailed user errors for unknown, stale, and non-ready files', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid fileAcknowledgeUpdateFailed should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const unknownResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query:
        'mutation { fileAcknowledgeUpdateFailed(fileIds: ["gid://shopify/MediaImage/999999"]) { files { id } userErrors { field message code } } }',
    });
    expect(unknownResponse.body.data.fileAcknowledgeUpdateFailed).toEqual({
      files: null,
      userErrors: [
        {
          field: ['fileIds'],
          message: 'File id gid://shopify/MediaImage/999999 does not exist.',
          code: 'FILE_DOES_NOT_EXIST',
        },
      ],
    });

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileCreate($files: [FileCreateInput!]!) { fileCreate(files: $files) { files { id fileStatus } userErrors { field message code } } }',
        variables: {
          files: [
            {
              contentType: 'IMAGE',
              originalSource: 'https://cdn.example.com/non-ready.jpg',
            },
          ],
        },
      });
    const fileId = createResponse.body.data.fileCreate.files[0].id as string;
    expect(createResponse.body.data.fileCreate.files[0].fileStatus).toBe('UPLOADED');

    const nonReadyResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileAcknowledgeUpdateFailed($fileIds: [ID!]!) { fileAcknowledgeUpdateFailed(fileIds: $fileIds) { files { id } userErrors { field message code } } }',
        variables: { fileIds: [fileId] },
      });
    expect(nonReadyResponse.body.data.fileAcknowledgeUpdateFailed).toEqual({
      files: null,
      userErrors: [
        {
          field: ['fileIds'],
          message: `File with id ${fileId} is not in the READY state.`,
          code: 'NON_READY_STATE',
        },
      ],
    });
    expect(store.getState().stagedState.files[fileId]).not.toHaveProperty('updateFailureAcknowledgedAt');

    await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileUpdate($files: [FileUpdateInput!]!) { fileUpdate(files: $files) { files { id fileStatus } userErrors { field message code } } }',
        variables: { files: [{ id: fileId, alt: 'Ready before delete' }] },
      });
    await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileDelete($fileIds: [ID!]!) { fileDelete(fileIds: $fileIds) { deletedFileIds userErrors { field message code } } }',
        variables: { fileIds: [fileId] },
      });

    const staleResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileAcknowledgeUpdateFailed($fileIds: [ID!]!) { fileAcknowledgeUpdateFailed(fileIds: $fileIds) { files { id } userErrors { field message code } } }',
        variables: { fileIds: [fileId] },
      });
    expect(staleResponse.body.data.fileAcknowledgeUpdateFailed).toEqual({
      files: null,
      userErrors: [
        {
          field: ['fileIds'],
          message: `File id ${fileId} does not exist.`,
          code: 'FILE_DOES_NOT_EXIST',
        },
      ],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages fileUpdate locally for Files API records without proxying upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('fileUpdate should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileCreate($files: [FileCreateInput!]!) { fileCreate(files: $files) { files { id alt filename fileStatus ... on MediaImage { image { url } } } userErrors { field message code } } }',
        variables: {
          files: [
            {
              alt: 'Original alt',
              contentType: 'IMAGE',
              filename: 'original.jpg',
              originalSource: 'https://cdn.example.com/original.jpg',
            },
          ],
        },
      });

    const fileId = createResponse.body.data.fileCreate.files[0].id as string;
    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileUpdate($files: [FileUpdateInput!]!) { fileUpdate(files: $files) { files { id alt filename fileStatus ... on MediaImage { image { url } } } userErrors { field message code } } }',
        variables: {
          files: [
            {
              id: fileId,
              alt: 'Updated alt',
              filename: 'updated.jpg',
              originalSource: 'https://cdn.example.com/updated.jpg',
            },
          ],
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.fileUpdate).toEqual({
      files: [
        {
          id: fileId,
          alt: 'Updated alt',
          filename: 'updated.jpg',
          fileStatus: 'READY',
          image: {
            url: 'https://cdn.example.com/updated.jpg',
          },
        },
      ],
      userErrors: [],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getState().stagedState.files[fileId]).toMatchObject({
      alt: 'Updated alt',
      filename: 'updated.jpg',
      originalSource: 'https://cdn.example.com/updated.jpg',
      imageUrl: 'https://cdn.example.com/updated.jpg',
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.at(-1)).toMatchObject({
      operationName: 'FileUpdate',
      query: expect.stringContaining('mutation FileUpdate'),
      variables: {
        files: [
          {
            id: fileId,
            alt: 'Updated alt',
            filename: 'updated.jpg',
            originalSource: 'https://cdn.example.com/updated.jpg',
          },
        ],
      },
      status: 'staged',
      interpreted: {
        primaryRootField: 'fileUpdate',
        capability: {
          domain: 'media',
          execution: 'stage-locally',
        },
      },
    });
  });

  it('rejects fileUpdate inputs that replace original and preview sources together', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid fileUpdate should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileCreate($files: [FileCreateInput!]!) { fileCreate(files: $files) { files { id alt filename fileStatus ... on MediaImage { image { url } } } userErrors { field message code } } }',
        variables: {
          files: [
            {
              alt: 'Original alt',
              contentType: 'IMAGE',
              filename: 'source-conflict.jpg',
              originalSource: 'https://cdn.example.com/source-conflict.jpg',
            },
          ],
        },
      });

    const fileId = createResponse.body.data.fileCreate.files[0].id as string;
    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileUpdate($files: [FileUpdateInput!]!) { fileUpdate(files: $files) { files { id alt filename fileStatus ... on MediaImage { image { url } } } userErrors { field message code } } }',
        variables: {
          files: [
            {
              id: fileId,
              originalSource: 'https://cdn.example.com/source-replacement.jpg',
              previewImageSource: 'https://cdn.example.com/preview-replacement.jpg',
            },
          ],
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.fileUpdate).toEqual({
      files: [],
      userErrors: [
        {
          field: ['files', '0'],
          message: 'Specify either originalSource or previewImageSource, not both.',
          code: 'INVALID',
        },
      ],
    });
    expect(store.getState().stagedState.files[fileId]).toMatchObject({
      originalSource: 'https://cdn.example.com/source-conflict.jpg',
      imageUrl: 'https://cdn.example.com/source-conflict.jpg',
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('updates product media through fileUpdate references while preserving downstream reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('fileUpdate product references should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const sourceProductResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Source media owner" }) { product { id } userErrors { field message } } }',
    });
    const targetProductResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Target media owner" }) { product { id } userErrors { field message } } }',
    });
    const sourceProductId = sourceProductResponse.body.data.productCreate.product.id as string;
    const targetProductId = targetProductResponse.body.data.productCreate.product.id as string;

    const createMediaResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation ProductCreateMedia($productId: ID!, $media: [CreateMediaInput!]!) { productCreateMedia(productId: $productId, media: $media) { media { id alt mediaContentType status } mediaUserErrors { field message } } }',
        variables: {
          productId: sourceProductId,
          media: [
            {
              alt: 'Original product media',
              mediaContentType: 'IMAGE',
              originalSource: 'https://cdn.example.com/product-media.jpg',
            },
          ],
        },
      });

    const mediaId = createMediaResponse.body.data.productCreateMedia.media[0].id as string;
    await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: 'query PromoteProcessing($id: ID!) { product(id: $id) { id media(first: 10) { nodes { id status } } } }',
        variables: { id: sourceProductId },
      });
    await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: 'query PromoteReady($id: ID!) { product(id: $id) { id media(first: 10) { nodes { id status } } } }',
        variables: { id: sourceProductId },
      });

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileUpdate($files: [FileUpdateInput!]!) { fileUpdate(files: $files) { files { id alt fileStatus ... on MediaImage { image { url } } } userErrors { field message code } } }',
        variables: {
          files: [
            {
              id: mediaId,
              alt: 'Updated shared media',
              referencesToAdd: [targetProductId],
            },
          ],
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.fileUpdate).toEqual({
      files: [
        {
          id: mediaId,
          alt: 'Updated shared media',
          fileStatus: 'READY',
          image: {
            url: 'https://cdn.example.com/product-media.jpg',
          },
        },
      ],
      userErrors: [],
    });

    const sourceReadResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'query SourceMedia($id: ID!) { product(id: $id) { id media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } } } } }',
        variables: { id: sourceProductId },
      });
    const targetReadResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'query TargetMedia($id: ID!) { product(id: $id) { id media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } } } } }',
        variables: { id: targetProductId },
      });

    expect(sourceReadResponse.body.data.product.media.nodes).toEqual([
      {
        id: mediaId,
        alt: 'Updated shared media',
        mediaContentType: 'IMAGE',
        status: 'READY',
        preview: {
          image: {
            url: 'https://cdn.example.com/product-media.jpg',
          },
        },
      },
    ]);
    expect(targetReadResponse.body.data.product.media.nodes).toEqual(sourceReadResponse.body.data.product.media.nodes);

    const removeReferenceResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileUpdate($files: [FileUpdateInput!]!) { fileUpdate(files: $files) { files { id alt fileStatus } userErrors { field message code } } }',
        variables: {
          files: [
            {
              id: mediaId,
              referencesToRemove: [sourceProductId],
            },
          ],
        },
      });
    const sourceAfterRemoveResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'query SourceAfterRemove($id: ID!) { product(id: $id) { id media(first: 10) { nodes { id alt } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: sourceProductId },
      });
    const targetAfterRemoveResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: 'query TargetAfterRemove($id: ID!) { product(id: $id) { id media(first: 10) { nodes { id alt } } } }',
        variables: { id: targetProductId },
      });

    expect(removeReferenceResponse.body.data.fileUpdate.userErrors).toEqual([]);
    expect(sourceAfterRemoveResponse.body.data.product.media).toEqual({
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    });
    expect(targetAfterRemoveResponse.body.data.product.media.nodes).toEqual([
      {
        id: mediaId,
        alt: 'Updated shared media',
      },
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages fileDelete locally for Files API records without proxying upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('fileDelete should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileCreate($files: [FileCreateInput!]!) { fileCreate(files: $files) { files { id alt } userErrors { field message code } } }',
        variables: {
          files: [
            {
              alt: 'Delete me',
              contentType: 'IMAGE',
              originalSource: 'https://cdn.example.com/delete-me.jpg',
            },
            {
              alt: 'Keep me',
              contentType: 'FILE',
              originalSource: 'https://cdn.example.com/keep-me.pdf',
            },
          ],
        },
      });

    const deletedFileId = createResponse.body.data.fileCreate.files[0].id as string;
    const keptFileId = createResponse.body.data.fileCreate.files[1].id as string;
    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileDelete($fileIds: [ID!]!) { fileDelete(fileIds: $fileIds) { deletedFileIds userErrors { field message code } } }',
        variables: { fileIds: [deletedFileId] },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.fileDelete).toEqual({
      deletedFileIds: [deletedFileId],
      userErrors: [],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(store.getState().stagedState.files).toEqual({
      [keptFileId]: expect.objectContaining({
        alt: 'Keep me',
        originalSource: 'https://cdn.example.com/keep-me.pdf',
      }),
    });
    expect(store.getState().stagedState.deletedFileIds).toEqual({
      [deletedFileId]: true,
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.at(-1)).toMatchObject({
      operationName: 'FileDelete',
      query: expect.stringContaining('mutation FileDelete'),
      variables: {
        fileIds: [deletedFileId],
      },
      status: 'staged',
      interpreted: {
        primaryRootField: 'fileDelete',
        capability: {
          domain: 'media',
          execution: 'stage-locally',
        },
      },
    });
  });

  it('returns Shopify-like fileDelete user errors without staging unknown file ids', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('invalid fileDelete should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const deleteResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query:
        'mutation { fileDelete(fileIds: ["gid://shopify/GenericFile/999999"]) { deletedFileIds userErrors { field message code } } }',
    });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.fileDelete).toEqual({
      deletedFileIds: null,
      userErrors: [
        {
          field: ['fileIds'],
          message: 'File id gid://shopify/GenericFile/999999 does not exist.',
          code: 'FILE_DOES_NOT_EXIST',
        },
      ],
    });
    expect(store.getState().stagedState.files).toEqual({});
    expect(store.getState().stagedState.deletedFileIds).toEqual({});
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages fileDelete locally and removes matching product media references from downstream reads', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('fileDelete should not hit upstream fetch');
    });
    const app = createApp(config).callback();

    const productResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Media file delete owner" }) { product { id } userErrors { field message } } }',
    });
    const productId = productResponse.body.data.productCreate.product.id as string;

    const createMediaResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation ProductCreateMedia($productId: ID!, $media: [CreateMediaInput!]!) { productCreateMedia(productId: $productId, media: $media) { media { id alt mediaContentType status } mediaUserErrors { field message } } }',
        variables: {
          productId,
          media: [
            {
              alt: 'Attached file image',
              mediaContentType: 'IMAGE',
              originalSource: 'https://cdn.example.com/attached-file-image.jpg',
            },
          ],
        },
      });

    const mediaId = createMediaResponse.body.data.productCreateMedia.media[0].id as string;
    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation FileDelete($fileIds: [ID!]!) { fileDelete(fileIds: $fileIds) { deletedFileIds userErrors { field message code } } }',
        variables: { fileIds: [mediaId] },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.fileDelete).toEqual({
      deletedFileIds: [mediaId],
      userErrors: [],
    });

    const downstreamMediaResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'query ProductMediaAfterFileDelete($id: ID!) { product(id: $id) { id media(first: 10) { nodes { id alt mediaContentType status } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }',
        variables: { id: productId },
      });

    expect(downstreamMediaResponse.status).toBe(200);
    expect(downstreamMediaResponse.body.data.product.media).toEqual({
      nodes: [],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: null,
        endCursor: null,
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
