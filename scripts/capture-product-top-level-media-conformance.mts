/* oxlint-disable no-console -- CLI recorder reports capture progress/results. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const requestDir = path.join('config', 'parity-requests', 'products');
const specDir = path.join('config', 'parity-specs', 'products');
const fixturePath = path.join(fixtureDir, 'product-top-level-media-parity.json');
const createRequestPath = path.join(requestDir, 'product-top-level-media-create.graphql');
const updateRequestPath = path.join(requestDir, 'product-top-level-media-update.graphql');
const readRequestPath = path.join(requestDir, 'product-top-level-media-read.graphql');
const specPath = path.join(specDir, 'product-top-level-media-parity.json');

const createTopLevelMediaDocument = `#graphql
mutation ProductTopLevelMediaCreate($product: ProductCreateInput!, $media: [CreateMediaInput!]) {
  productCreate(product: $product, media: $media) {
    product {
      id
      title
      media(first: 10) {
        nodes {
          __typename
          id
          alt
          mediaContentType
          status
          preview {
            image {
              url
            }
          }
          ... on MediaImage {
            image {
              url
            }
          }
          ... on ExternalVideo {
            originUrl
            embedUrl
          }
        }
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const updateTopLevelMediaDocument = `#graphql
mutation ProductTopLevelMediaUpdate($product: ProductUpdateInput!, $media: [CreateMediaInput!]) {
  productUpdate(product: $product, media: $media) {
    product {
      id
      title
      media(first: 10) {
        nodes {
          __typename
          id
          alt
          mediaContentType
          status
          preview {
            image {
              url
            }
          }
          ... on MediaImage {
            image {
              url
            }
          }
          ... on ExternalVideo {
            originUrl
            embedUrl
          }
        }
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const readTopLevelMediaDocument = `#graphql
query ProductTopLevelMediaRead($id: ID!) {
  product(id: $id) {
    id
    title
    media(first: 10) {
      nodes {
        __typename
        id
        alt
        mediaContentType
        status
        preview {
          image {
            url
          }
        }
        ... on MediaImage {
          image {
            url
          }
        }
        ... on ExternalVideo {
          originUrl
          embedUrl
        }
      }
    }
  }
}
`;

const deleteProductDocument = `#graphql
mutation ProductTopLevelMediaCleanup($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}
`;

function responseData(response: { payload?: unknown }): Record<string, unknown> {
  const payload = response.payload;
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) {
    throw new Error(`Expected GraphQL response payload object, got ${JSON.stringify(payload)}`);
  }
  const data = (payload as Record<string, unknown>)['data'];
  if (!data || typeof data !== 'object' || Array.isArray(data)) {
    throw new Error(`Expected GraphQL response data object, got ${JSON.stringify(payload)}`);
  }
  return data as Record<string, unknown>;
}

function productIdFromCreate(response: { payload?: unknown }, label: string): string {
  const data = responseData(response);
  const root = data['productCreate'];
  if (!root || typeof root !== 'object' || Array.isArray(root)) {
    throw new Error(`${label} did not return productCreate payload`);
  }
  const product = (root as Record<string, unknown>)['product'];
  if (!product || typeof product !== 'object' || Array.isArray(product)) {
    throw new Error(`${label} did not return a product: ${JSON.stringify(root, null, 2)}`);
  }
  const id = (product as Record<string, unknown>)['id'];
  if (typeof id !== 'string') {
    throw new Error(`${label} product id was not a string: ${JSON.stringify(product, null, 2)}`);
  }
  return id;
}

function assertNoUserErrors(response: { payload?: unknown }, rootName: 'productCreate' | 'productUpdate'): void {
  const data = responseData(response);
  const root = data[rootName];
  if (!root || typeof root !== 'object' || Array.isArray(root)) {
    throw new Error(`${rootName} did not return a payload`);
  }
  const userErrors = (root as Record<string, unknown>)['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }
  throw new Error(`${rootName} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
}

function productIdFromPrimaryProxyPath() {
  return { fromPrimaryProxyPath: '$.data.productCreate.product.id' };
}

function productIdDifference(pathExpression: string) {
  return {
    path: pathExpression,
    matcher: 'shopify-gid:Product',
    reason: 'The proxy generates a stable synthetic Product GID for the local staged session.',
  };
}

function mediaIdDifference(pathExpression: string, typeName: 'MediaImage' | 'ExternalVideo') {
  return {
    path: pathExpression,
    matcher: `shopify-gid:${typeName}`,
    reason: `The proxy generates a stable synthetic ${typeName} GID for the local staged session.`,
  };
}

function commonProductMediaDifferences(rootPath: string) {
  return [
    productIdDifference(`${rootPath}.id`),
    mediaIdDifference(`${rootPath}.media.nodes[0].id`, 'MediaImage'),
    mediaIdDifference(`${rootPath}.media.nodes[1].id`, 'ExternalVideo'),
  ];
}

function buildSpec() {
  return {
    scenarioId: 'product-top-level-media-parity',
    operationNames: ['productCreate', 'productUpdate'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'user-errors-parity', 'mutation-no-write-parity', 'downstream-read-parity'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['src/proxy/media_products_saved_searches.rs'],
    proxyRequest: {
      documentPath: createRequestPath,
      apiVersion,
      variablesCapturePath: '$.validCreate.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Captured Shopify productCreate/productUpdate top-level media argument behavior with a valid image source, valid external-video source, invalid image source, and disposable product cleanup. The proxy request sequence uses only public Admin GraphQL mutations/queries.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'valid-create-image-media-payload',
          capturePath: '$.validCreate.response.payload.data',
          proxyPath: '$.data',
          expectedDifferences: [
            productIdDifference('$.productCreate.product.id'),
            mediaIdDifference('$.productCreate.product.media.nodes[0].id', 'MediaImage'),
          ],
        },
        {
          name: 'read-after-valid-create-image-processing',
          capturePath: '$.readAfterValidCreate.response.payload.data.product',
          proxyPath: '$.data.product',
          proxyRequest: {
            documentPath: readRequestPath,
            apiVersion,
            variables: {
              id: productIdFromPrimaryProxyPath(),
            },
          },
          expectedDifferences: [productIdDifference('$.id'), mediaIdDifference('$.media.nodes[0].id', 'MediaImage')],
        },
        {
          name: 'valid-update-external-video-appends-media',
          capturePath: '$.validUpdate.response.payload.data.productUpdate.product',
          proxyPath: '$.data.productUpdate.product',
          proxyRequest: {
            documentPath: updateRequestPath,
            apiVersion,
            variables: {
              product: {
                id: productIdFromPrimaryProxyPath(),
                title: { fromCapturePath: '$.validUpdate.variables.product.title' },
              },
              media: { fromCapturePath: '$.validUpdate.variables.media' },
            },
          },
          expectedDifferences: commonProductMediaDifferences('$'),
        },
        {
          name: 'read-after-valid-update-preserves-and-appends-media',
          capturePath: '$.readAfterValidUpdate.response.payload.data.product',
          proxyPath: '$.data.product',
          proxyRequest: {
            documentPath: readRequestPath,
            apiVersion,
            variables: {
              id: productIdFromPrimaryProxyPath(),
            },
          },
          expectedDifferences: commonProductMediaDifferences('$'),
        },
        {
          name: 'invalid-update-media-user-errors-and-unchanged-product',
          capturePath: '$.invalidUpdate.response.payload.data.productUpdate',
          proxyPath: '$.data.productUpdate',
          proxyRequest: {
            documentPath: updateRequestPath,
            apiVersion,
            variables: {
              product: {
                id: productIdFromPrimaryProxyPath(),
                title: { fromCapturePath: '$.invalidUpdate.variables.product.title' },
              },
              media: { fromCapturePath: '$.invalidUpdate.variables.media' },
            },
          },
          expectedDifferences: [
            productIdDifference('$.product.id'),
            mediaIdDifference('$.product.media.nodes[0].id', 'MediaImage'),
            mediaIdDifference('$.product.media.nodes[1].id', 'ExternalVideo'),
          ],
        },
        {
          name: 'read-after-invalid-update-remains-unchanged',
          capturePath: '$.readAfterInvalidUpdate.response.payload.data.product',
          proxyPath: '$.data.product',
          proxyRequest: {
            documentPath: readRequestPath,
            apiVersion,
            variables: {
              id: productIdFromPrimaryProxyPath(),
            },
          },
          expectedDifferences: commonProductMediaDifferences('$'),
        },
        {
          name: 'invalid-create-media-null-product',
          capturePath: '$.invalidCreate.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: createRequestPath,
            apiVersion,
            variablesCapturePath: '$.invalidCreate.variables',
          },
        },
      ],
    },
  };
}

await mkdir(fixtureDir, { recursive: true });
await mkdir(requestDir, { recursive: true });
await mkdir(specDir, { recursive: true });

await writeFile(createRequestPath, createTopLevelMediaDocument, 'utf8');
await writeFile(updateRequestPath, updateTopLevelMediaDocument, 'utf8');
await writeFile(readRequestPath, readTopLevelMediaDocument, 'utf8');

const runId = `${Date.now()}`;
const createdProductIds: string[] = [];

try {
  const validCreateVariables = {
    product: {
      title: `Product top-level media ${runId}`,
      status: 'DRAFT',
    },
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: 'https://placehold.co/640x480/png',
        alt: 'Top level image',
      },
    ],
  };
  const validCreate = await runGraphqlRequest(createTopLevelMediaDocument, validCreateVariables);
  assertNoUserErrors(validCreate, 'productCreate');
  const productId = productIdFromCreate(validCreate, 'valid productCreate');
  createdProductIds.push(productId);

  const readAfterValidCreate = await runGraphqlRequest(readTopLevelMediaDocument, { id: productId });

  const validUpdateVariables = {
    product: {
      id: productId,
      title: `Product top-level media updated ${runId}`,
    },
    media: [
      {
        mediaContentType: 'EXTERNAL_VIDEO',
        originalSource: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
        alt: 'Top level external video',
      },
    ],
  };
  const validUpdate = await runGraphqlRequest(updateTopLevelMediaDocument, validUpdateVariables);
  assertNoUserErrors(validUpdate, 'productUpdate');
  const readAfterValidUpdate = await runGraphqlRequest(readTopLevelMediaDocument, { id: productId });

  const invalidUpdateVariables = {
    product: {
      id: productId,
      title: `Product top-level media invalid update ${runId}`,
    },
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: 'not-a-url',
        alt: 'Invalid update source',
      },
    ],
  };
  const invalidUpdate = await runGraphqlRequest(updateTopLevelMediaDocument, invalidUpdateVariables);
  const readAfterInvalidUpdate = await runGraphqlRequest(readTopLevelMediaDocument, { id: productId });

  const invalidCreateVariables = {
    product: {
      title: `Product top-level media invalid create ${runId}`,
      status: 'DRAFT',
    },
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: 'not-a-url',
        alt: 'Invalid create source',
      },
    ],
  };
  const invalidCreate = await runGraphqlRequest(createTopLevelMediaDocument, invalidCreateVariables);
  const invalidCreateProduct = (responseData(invalidCreate)['productCreate'] as { product?: { id?: unknown } | null })
    ?.product;
  if (invalidCreateProduct && typeof invalidCreateProduct.id === 'string') {
    createdProductIds.push(invalidCreateProduct.id);
  }

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    operations: ['productCreate', 'productUpdate'],
    setup: {
      imageSource: 'https://placehold.co/640x480/png',
      externalVideoSource: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
      invalidSource: 'not-a-url',
      cleanup: 'Deletes disposable products created during capture.',
    },
    validCreate: {
      variables: validCreateVariables,
      response: validCreate,
    },
    readAfterValidCreate: {
      variables: { id: productId },
      response: readAfterValidCreate,
    },
    validUpdate: {
      variables: validUpdateVariables,
      response: validUpdate,
    },
    readAfterValidUpdate: {
      variables: { id: productId },
      response: readAfterValidUpdate,
    },
    invalidUpdate: {
      variables: invalidUpdateVariables,
      response: invalidUpdate,
    },
    readAfterInvalidUpdate: {
      variables: { id: productId },
      response: readAfterInvalidUpdate,
    },
    invalidCreate: {
      variables: invalidCreateVariables,
      response: invalidCreate,
    },
    notes:
      'Valid productCreate top-level media returns image media in the mutation payload; immediate product reads show that media processing. Valid productUpdate appends an external video while preserving existing image media. Invalid originalSource returns product userErrors and does not stage create/update changes.',
  };

  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  await writeFile(specPath, `${JSON.stringify(buildSpec(), null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        fixturePath,
        specPath,
        requestPaths: [createRequestPath, updateRequestPath, readRequestPath],
        productId,
      },
      null,
      2,
    ),
  );
} finally {
  for (const id of createdProductIds.reverse()) {
    try {
      await runGraphql(deleteProductDocument, { input: { id } });
    } catch {
      // Best-effort cleanup for disposable conformance products.
    }
  }
}
