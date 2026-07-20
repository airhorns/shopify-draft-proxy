/* oxlint-disable no-console -- capture scripts report progress and failures to the CLI. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlVariables = Record<string, unknown>;
type GraphqlPayload = {
  data?: Record<string, unknown>;
  errors?: unknown;
  extensions?: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputPath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'media',
  'media-file-mutation-first-lifecycle.json',
);
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphql: (query: string, variables?: GraphqlVariables) => Promise<GraphqlPayload>;
};

const productCreateDocument = `mutation MediaFileLifecycleProductCreate($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product { id title }
    userErrors { field message }
  }
}`;
const productCreateMediaDocument = `mutation MediaFileLifecycleProductCreateMedia($productId: ID!, $media: [CreateMediaInput!]!) {
  productCreateMedia(productId: $productId, media: $media) {
    media { id alt mediaContentType status }
    mediaUserErrors { field message code }
  }
}`;
const fileReadyDocument = `query MediaFileLifecycleReady($id: ID!) {
  node(id: $id) {
    ... on File { id fileStatus }
  }
}`;
const fileTargetHydrateDocument = `query MediaFileTargetHydrate($fileIds: [ID!]!) {
  nodes(ids: $fileIds) {
    id
    __typename
    ... on File {
      alt
      createdAt
      fileStatus
    }
    ... on MediaImage {
      image { url width height }
      preview { image { url width height } }
    }
    ... on GenericFile {
      url
    }
  }
}`;
const acknowledgeDocument = `mutation MediaFileMutationFirstAcknowledge($fileIds: [ID!]!) {
  fileAcknowledgeUpdateFailed(fileIds: $fileIds) {
    files { id alt fileStatus }
    userErrors { field message code }
  }
}`;
const productOwnersHydrateDocument = `query MediaProductOwnersHydrate($ids: [ID!]!) {
  nodes(ids: $ids) {
    ... on Product {
      id
      title
      handle
      status
      media(first: 50) {
        nodes {
          id
          __typename
          alt
          mediaContentType
          status
          preview { image { url width height } }
          ... on MediaImage { image { url width height } }
        }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
      variants(first: 50) {
        nodes {
          id
          title
          media(first: 10) {
            nodes { id alt mediaContentType }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
    }
  }
}`;
const deleteDocument = `mutation MediaFileMutationFirstDelete($fileIds: [ID!]!) {
  fileDelete(fileIds: $fileIds) {
    deletedFileIds
    userErrors { field message code }
  }
}`;
const downstreamReadDocument = `query MediaFileMutationFirstDownstream($id: ID!) {
  product(id: $id) {
    id
    media(first: 10) { nodes { id } }
  }
}`;
const productDeleteDocument = `mutation MediaFileLifecycleProductDelete($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors { field message }
  }
}`;

function requireNoTopLevelErrors(label: string, response: GraphqlPayload): void {
  if (response.errors === undefined) return;
  throw new Error(`${label} returned top-level errors: ${JSON.stringify(response.errors, null, 2)}`);
}

function requireNoUserErrors(label: string, response: GraphqlPayload, pointer: string[]): void {
  let value: unknown = response.data;
  for (const segment of pointer) {
    value = typeof value === 'object' && value !== null ? (value as Record<string, unknown>)[segment] : undefined;
  }
  if (Array.isArray(value) && value.length === 0) return;
  throw new Error(`${label} returned user errors: ${JSON.stringify(value ?? null, null, 2)}`);
}

function requireString(label: string, value: unknown): string {
  if (typeof value === 'string' && value.length > 0) return value;
  throw new Error(`${label} was not a non-empty string.`);
}

async function waitUntilReady(fileId: string): Promise<GraphqlPayload> {
  for (let attempt = 0; attempt < 30; attempt += 1) {
    const response = await runGraphql(fileReadyDocument, { id: fileId });
    requireNoTopLevelErrors('file ready poll', response);
    const node = response.data?.node as Record<string, unknown> | null | undefined;
    if (node?.fileStatus === 'READY') return response;
    await delay(2_000);
  }
  throw new Error(`File ${fileId} did not reach READY within the capture timeout.`);
}

const runId = Date.now();
const productCreateVariables = {
  product: {
    title: `Media file mutation-first lifecycle ${runId}`,
    status: 'DRAFT',
  },
};
let productId: string | undefined;
let fileId: string | undefined;
let deletedFile = false;

try {
  const productCreateResponse = await runGraphql(productCreateDocument, productCreateVariables);
  requireNoTopLevelErrors('productCreate', productCreateResponse);
  requireNoUserErrors('productCreate', productCreateResponse, ['productCreate', 'userErrors']);
  productId = requireString(
    'productCreate.product.id',
    ((productCreateResponse.data?.productCreate as Record<string, unknown>)?.product as Record<string, unknown>)?.id,
  );

  const productCreateMediaVariables = {
    productId,
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: 'https://placehold.co/640x480/png',
        alt: `Mutation-first lifecycle ${runId}`,
      },
    ],
  };
  const productCreateMediaResponse = await runGraphql(productCreateMediaDocument, productCreateMediaVariables);
  requireNoTopLevelErrors('productCreateMedia', productCreateMediaResponse);
  requireNoUserErrors('productCreateMedia', productCreateMediaResponse, ['productCreateMedia', 'mediaUserErrors']);
  const media = (productCreateMediaResponse.data?.productCreateMedia as Record<string, unknown>)?.media as
    | Array<Record<string, unknown>>
    | undefined;
  fileId = requireString('productCreateMedia.media[0].id', media?.[0]?.id);

  const readyResponse = await waitUntilReady(fileId);
  const targetVariables = { fileIds: [fileId] };
  const targetResponse = await runGraphql(fileTargetHydrateDocument, targetVariables);
  requireNoTopLevelErrors('MediaFileTargetHydrate', targetResponse);

  const acknowledgeResponse = await runGraphql(acknowledgeDocument, targetVariables);
  requireNoTopLevelErrors('fileAcknowledgeUpdateFailed', acknowledgeResponse);
  requireNoUserErrors('fileAcknowledgeUpdateFailed', acknowledgeResponse, [
    'fileAcknowledgeUpdateFailed',
    'userErrors',
  ]);

  const ownerVariables = { ids: [productId] };
  const ownerResponse = await runGraphql(productOwnersHydrateDocument, ownerVariables);
  requireNoTopLevelErrors('MediaProductOwnersHydrate', ownerResponse);

  const deleteResponse = await runGraphql(deleteDocument, targetVariables);
  requireNoTopLevelErrors('fileDelete', deleteResponse);
  requireNoUserErrors('fileDelete', deleteResponse, ['fileDelete', 'userErrors']);
  deletedFile = true;

  const downstreamVariables = { id: productId };
  const downstreamResponse = await runGraphql(downstreamReadDocument, downstreamVariables);
  requireNoTopLevelErrors('product read after fileDelete', downstreamResponse);

  const capture = {
    setup: {
      productCreate: { variables: productCreateVariables, response: productCreateResponse },
      productCreateMedia: {
        variables: productCreateMediaVariables,
        response: productCreateMediaResponse,
      },
      readyPoll: { variables: { id: fileId }, response: readyResponse },
    },
    acknowledge: { variables: targetVariables, response: acknowledgeResponse },
    delete: { variables: targetVariables, response: deleteResponse },
    downstreamRead: { variables: downstreamVariables, response: downstreamResponse },
    upstreamCalls: [
      {
        operationName: 'MediaFileTargetHydrate',
        variables: targetVariables,
        query: fileTargetHydrateDocument,
        response: { status: 200, body: targetResponse },
      },
      {
        operationName: 'MediaProductOwnersHydrate',
        variables: ownerVariables,
        query: productOwnersHydrateDocument,
        response: { status: 200, body: ownerResponse },
      },
    ],
  };

  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (fileId !== undefined && !deletedFile) {
    try {
      await runGraphql(deleteDocument, { fileIds: [fileId] });
    } catch (error) {
      console.warn(`Best-effort file cleanup failed: ${String(error)}`);
    }
  }
  if (productId !== undefined) {
    try {
      await runGraphql(productDeleteDocument, { input: { id: productId } });
    } catch (error) {
      console.warn(`Best-effort product cleanup failed: ${String(error)}`);
    }
  }
}
