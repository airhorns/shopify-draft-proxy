/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-parent-observation-preservation';
const requestDirectory = path.join('config', 'parity-requests', 'localization');

const productCreateMutation = `#graphql
  mutation LocalizationParentObservationProductCreate(
    $product: ProductCreateInput!
    $media: [CreateMediaInput!]
  ) {
    productCreate(product: $product, media: $media) {
      product { id }
      userErrors { field message }
    }
  }
`;

const collectionCreateMutation = `#graphql
  mutation LocalizationParentObservationCollectionCreate($input: CollectionInput!) {
    collectionCreate(input: $input) {
      collection { id }
      userErrors { field message }
    }
  }
`;

const productMediaReadyQuery = `#graphql
  query LocalizationParentObservationMediaReady($productId: ID!) {
    product(id: $productId) {
      media(first: 10) {
        nodes {
          id
          mediaContentType
          status
        }
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation LocalizationParentObservationProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

const collectionDeleteMutation = `#graphql
  mutation LocalizationParentObservationCollectionDelete($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors { field message }
    }
  }
`;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function dataObject(payload: ConformanceGraphqlPayload<unknown>): JsonRecord {
  if (!isRecord(payload.data)) {
    throw new Error(`Expected GraphQL data object, got ${JSON.stringify(payload)}`);
  }
  return payload.data;
}

function payloadObject(payload: ConformanceGraphqlPayload<unknown>, root: string): JsonRecord {
  const value = dataObject(payload)[root];
  if (!isRecord(value)) {
    throw new Error(`Expected data.${root} object, got ${JSON.stringify(payload)}`);
  }
  return value;
}

function assertNoUserErrors(payload: ConformanceGraphqlPayload<unknown>, root: string): JsonRecord {
  const value = payloadObject(payload, root);
  const errors = value['userErrors'];
  if (!Array.isArray(errors) || errors.length !== 0) {
    throw new Error(`${root} returned userErrors: ${JSON.stringify(errors)}`);
  }
  return value;
}

function resourceId(payload: ConformanceGraphqlPayload<unknown>, root: string, resource: string): string {
  const value = assertNoUserErrors(payload, root)[resource];
  if (!isRecord(value) || typeof value['id'] !== 'string') {
    throw new Error(`${root}.${resource}.id was not returned: ${JSON.stringify(payload)}`);
  }
  return value['id'];
}

function randomSuffix(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

async function readRequest(filename: string): Promise<string> {
  return readFile(path.join(requestDirectory, filename), 'utf8');
}

async function waitForReadyMedia(options: {
  runGraphql: (query: string, variables?: JsonRecord) => Promise<ConformanceGraphqlPayload<unknown>>;
  productId: string;
}): Promise<ConformanceGraphqlPayload<unknown>> {
  let lastPayload: ConformanceGraphqlPayload<unknown> | null = null;
  for (let attempt = 0; attempt < 15; attempt += 1) {
    lastPayload = await options.runGraphql(productMediaReadyQuery, { productId: options.productId });
    const product = dataObject(lastPayload)['product'];
    const media = isRecord(product) ? product['media'] : null;
    const nodes = isRecord(media) ? media['nodes'] : null;
    if (
      Array.isArray(nodes) &&
      nodes.length > 0 &&
      nodes.every((node) => isRecord(node) && node['status'] === 'READY')
    ) {
      return lastPayload;
    }
    if (Array.isArray(nodes) && nodes.some((node) => isRecord(node) && node['status'] === 'FAILED')) {
      throw new Error(`Product media failed processing: ${JSON.stringify(lastPayload)}`);
    }

    await new Promise<void>((resolve) => {
      setTimeout(resolve, 2_000);
    });
  }

  throw new Error(`Timed out waiting for Product media to become READY: ${JSON.stringify(lastPayload)}`);
}

async function bestEffortCleanup(options: {
  runGraphql: (query: string, variables?: JsonRecord) => Promise<ConformanceGraphqlPayload<unknown>>;
  productId: string | null;
  collectionId: string | null;
}): Promise<JsonRecord> {
  const cleanup: JsonRecord = {};
  if (options.collectionId !== null) {
    try {
      cleanup['collectionDelete'] = await options.runGraphql(collectionDeleteMutation, {
        input: { id: options.collectionId },
      });
    } catch (error: unknown) {
      cleanup['collectionDeleteError'] = String(error);
    }
  }
  if (options.productId !== null) {
    try {
      cleanup['productDelete'] = await options.runGraphql(productDeleteMutation, {
        input: { id: options.productId },
      });
    } catch (error: unknown) {
      cleanup['productDeleteError'] = String(error);
    }
  }
  return cleanup;
}

async function main(): Promise<void> {
  const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
  if (apiVersion !== '2026-04') {
    throw new Error(`Expected SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${apiVersion}`);
  }
  const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
  const { runGraphql } = createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(adminAccessToken),
  });
  const [
    sourceReadQuery,
    canonicalNodesQuery,
    disjointReadQuery,
    productUpdateMutation,
    collectionUpdateMutation,
    finalReadQuery,
  ] = await Promise.all([
    readRequest('localization-parent-observation-source-read.graphql'),
    readRequest('localization-parent-observation-canonical-nodes.graphql'),
    readRequest('localization-parent-observation-disjoint-read.graphql'),
    readRequest('localization-parent-observation-product-update.graphql'),
    readRequest('localization-parent-observation-collection-update.graphql'),
    readRequest('localization-parent-observation-final-read.graphql'),
  ]);
  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'localization');
  const outputPath = path.join(outputDir, `${scenarioId}.json`);
  const suffix = randomSuffix();
  const productInput = {
    title: `Localization parent product ${suffix}`,
    handle: `localization-parent-product-${suffix}`.replace(/[^a-z0-9-]/gu, '-'),
    descriptionHtml: `<p>Localization parent product body ${suffix}</p>`,
    vendor: `Localization Vendor ${suffix}`,
    productType: `Localization Type ${suffix}`,
    tags: ['localization-parent', `capture-${suffix}`],
    templateSuffix: 'localization-parent-product',
    seo: {
      title: `Localization product SEO ${suffix}`,
      description: `Localization product SEO description ${suffix}`,
    },
    status: 'ARCHIVED',
  };
  const mediaInput = [
    {
      mediaContentType: 'EXTERNAL_VIDEO',
      originalSource: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
      alt: `Localization parent media ${suffix}`,
    },
  ];
  const updatedProductTitle = `Localization parent product updated ${suffix}`;
  const updatedCollectionTitle = `Localization parent collection updated ${suffix}`;

  let productId: string | null = null;
  let collectionId: string | null = null;
  let cleanup: JsonRecord = {};

  try {
    const productCreateVariables = { product: productInput, media: mediaInput };
    const productCreate = await runGraphql(productCreateMutation, productCreateVariables);
    productId = resourceId(productCreate, 'productCreate', 'product');
    const mediaReadyRead = await waitForReadyMedia({ runGraphql, productId });

    const collectionCreateVariables = {
      input: {
        title: `Localization parent collection ${suffix}`,
        handle: `localization-parent-collection-${suffix}`.replace(/[^a-z0-9-]/gu, '-'),
        descriptionHtml: `<p>Localization parent collection body ${suffix}</p>`,
        templateSuffix: 'localization-parent-collection',
        sortOrder: 'MANUAL',
        seo: {
          title: `Localization collection SEO ${suffix}`,
          description: `Localization collection SEO description ${suffix}`,
        },
        products: [productId],
      },
    };
    const collectionCreate = await runGraphql(collectionCreateMutation, collectionCreateVariables);
    collectionId = resourceId(collectionCreate, 'collectionCreate', 'collection');

    const identityVariables = { productId, collectionId };
    const nodeVariables = { ids: [collectionId, productId] };
    const sourceRead = await runGraphql(sourceReadQuery, identityVariables);
    const disjointRead = await runGraphql(disjointReadQuery, identityVariables);
    const sourceAfterDisjoint = await runGraphql(sourceReadQuery, identityVariables);
    const canonicalNodes = await runGraphql(canonicalNodesQuery, nodeVariables);

    const productUpdateVariables = {
      product: { id: productId, title: updatedProductTitle },
    };
    const productUpdate = await runGraphql(productUpdateMutation, productUpdateVariables);
    assertNoUserErrors(productUpdate, 'productUpdate');

    const collectionUpdateVariables = {
      input: { id: collectionId, title: updatedCollectionTitle },
    };
    const collectionUpdate = await runGraphql(collectionUpdateMutation, collectionUpdateVariables);
    assertNoUserErrors(collectionUpdate, 'collectionUpdate');

    const finalReadVariables = { ...identityVariables, ...nodeVariables };
    const finalRead = await runGraphql(finalReadQuery, finalReadVariables);

    cleanup = await bestEffortCleanup({ runGraphql, productId, collectionId });
    productId = null;
    collectionId = null;

    const capture = {
      scenarioId,
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      setup: {
        productCreate: { variables: productCreateVariables, response: productCreate },
        mediaReadyRead: { variables: { productId }, response: mediaReadyRead },
        collectionCreate: { variables: collectionCreateVariables, response: collectionCreate },
      },
      sourceRead: { request: { variables: identityVariables }, response: sourceRead },
      sourceAfterDisjoint: { request: { variables: identityVariables }, response: sourceAfterDisjoint },
      canonicalNodes: { request: { variables: nodeVariables }, response: canonicalNodes },
      disjointRead: { request: { variables: identityVariables }, response: disjointRead },
      productUpdate: { request: { variables: productUpdateVariables }, response: productUpdate },
      collectionUpdate: { request: { variables: collectionUpdateVariables }, response: collectionUpdate },
      finalRead: { request: { variables: finalReadVariables }, response: finalRead },
      cleanup,
      upstreamCalls: [
        {
          operationName: 'LocalizationParentObservationSourceRead',
          variables: identityVariables,
          query: sourceReadQuery,
          response: { status: 200, body: sourceRead },
        },
        {
          operationName: 'LocalizationParentObservationDisjointRead',
          variables: identityVariables,
          query: disjointReadQuery,
          response: { status: 200, body: disjointRead },
        },
        {
          operationName: 'LocalizationParentObservationCanonicalNodes',
          variables: nodeVariables,
          query: canonicalNodesQuery,
          response: { status: 200, body: canonicalNodes },
        },
      ],
    };

    await mkdir(outputDir, { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
    console.log(JSON.stringify({ ok: true, outputPath, storeDomain, apiVersion }, null, 2));
  } finally {
    if (productId !== null || collectionId !== null) {
      cleanup = await bestEffortCleanup({ runGraphql, productId, collectionId });
      console.log(`Cleanup after failure: ${JSON.stringify(cleanup)}`);
    }
  }
}

await main();
