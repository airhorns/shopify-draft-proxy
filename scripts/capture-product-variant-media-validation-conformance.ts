/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureStep = {
  variables: Record<string, unknown>;
  response: unknown;
};

type RawCapture = {
  label: string;
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-variant-media-validation.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateDocument = await readGraphqlDocument(
  'config/parity-requests/products/product-variant-media-validation-product-create.graphql',
);
const productCreateMediaDocument = await readGraphqlDocument(
  'config/parity-requests/products/product-variant-media-validation-product-create-media.graphql',
);
const productUpdateMediaDocument = await readGraphqlDocument(
  'config/parity-requests/products/product-variant-media-validation-product-update-media.graphql',
);
const appendDocument = await readGraphqlDocument(
  'config/parity-requests/products/product-variant-media-validation-append.graphql',
);
const detachDocument = await readGraphqlDocument(
  'config/parity-requests/products/product-variant-media-validation-detach.graphql',
);

const productDeleteMutation = `#graphql
  mutation ProductVariantMediaValidationProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

const productMediaReadQuery = `#graphql
  query ProductVariantMediaValidationMediaRead($productId: ID!) {
    product(id: $productId) {
      id
      media(first: 10) {
        nodes {
          id
          alt
          mediaContentType
          status
        }
      }
    }
  }
`;

async function readGraphqlDocument(documentPath: string): Promise<string> {
  return readFile(documentPath, 'utf8');
}

function readObject(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${label} was not an object.`);
  }
  return value as Record<string, unknown>;
}

function readArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${label} was not an array.`);
  }
  return value;
}

function readData(result: ConformanceGraphqlResult, label: string): Record<string, unknown> {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
  return readObject(result.payload.data, `${label}.data`);
}

async function runStep(label: string, query: string, variables: Record<string, unknown> = {}): Promise<CaptureStep> {
  const result = await runGraphqlRaw(query, variables);
  readData(result, label);
  return { variables, response: result.payload };
}

async function runRaw(label: string, query: string, variables: Record<string, unknown> = {}): Promise<RawCapture> {
  const result = await runGraphqlRaw(query, variables);
  readData(result, label);
  return { label, status: result.status, response: result.payload };
}

function readMutationPayload(step: CaptureStep, root: string): Record<string, unknown> {
  return readObject(
    readObject(readObject(step.response, `${root}.response`)['data'], `${root}.response.data`)[root],
    `${root}.payload`,
  );
}

function readCreatedProduct(step: CaptureStep): Record<string, unknown> {
  return readObject(readMutationPayload(step, 'productCreate')['product'], 'productCreate.product');
}

function readId(source: Record<string, unknown>, label: string): string {
  if (typeof source['id'] !== 'string') {
    throw new Error(`${label} did not include an id.`);
  }
  return source['id'];
}

function readFirstVariantId(product: Record<string, unknown>): string {
  const variants = readObject(product['variants'], 'product.variants');
  const nodes = readArray(variants['nodes'], 'product.variants.nodes');
  const firstVariant = readObject(nodes[0], 'product.variants.nodes[0]');
  return readId(firstVariant, 'product first variant');
}

function readCreatedMediaId(step: CaptureStep, label: string): string {
  const media = readArray(readMutationPayload(step, 'productCreateMedia')['media'], `${label}.media`);
  const firstMedia = readObject(media[0], `${label}.media[0]`);
  return readId(firstMedia, `${label}.media[0]`);
}

function readUserErrors(step: CaptureStep, root: string): unknown[] {
  return readArray(readMutationPayload(step, root)['userErrors'], `${root}.userErrors`);
}

function assertUserErrors(step: CaptureStep, root: string, label: string): void {
  const userErrors = readUserErrors(step, root);
  if (userErrors.length === 0) {
    throw new Error(`${label} unexpectedly returned no userErrors.`);
  }
}

function mediaStatuses(readStep: CaptureStep): string[] {
  const data = readObject(readStep.response, 'mediaRead.response');
  const product = readObject(readObject(data['data'], 'mediaRead.data')['product'], 'mediaRead.product');
  const media = readObject(product['media'], 'mediaRead.product.media');
  return readArray(media['nodes'], 'mediaRead.product.media.nodes').map((node, index) => {
    const mediaNode = readObject(node, `mediaRead.product.media.nodes[${index}]`);
    if (typeof mediaNode['status'] !== 'string') {
      throw new Error(`mediaRead.product.media.nodes[${index}].status was not a string.`);
    }
    return mediaNode['status'];
  });
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForReadyMedia(productId: string, expectedCount: number): Promise<CaptureStep> {
  let latestRead: CaptureStep | null = null;
  for (let attempt = 0; attempt < 12; attempt += 1) {
    if (attempt > 0) {
      await delay(5000);
    }
    latestRead = await runStep('media ready read', productMediaReadQuery, { productId });
    const statuses = mediaStatuses(latestRead);
    if (statuses.length >= expectedCount && statuses.slice(0, expectedCount).every((status) => status === 'READY')) {
      return latestRead;
    }
  }

  throw new Error(`Timed out waiting for ${expectedCount} product media item(s) to become READY.`);
}

const suffix = Date.now().toString(36);
const cleanup: RawCapture[] = [];
const productIds: string[] = [];
let fixturePayload: Record<string, unknown> | null = null;

try {
  const createBaseProduct = await runStep('base product setup', productCreateDocument, {
    product: { title: `Product variant media validation base ${suffix}`, status: 'DRAFT' },
  });
  const baseProduct = readCreatedProduct(createBaseProduct);
  const baseProductId = readId(baseProduct, 'base product');
  const baseVariantId = readFirstVariantId(baseProduct);
  productIds.push(baseProductId);

  const createOtherProduct = await runStep('other product setup', productCreateDocument, {
    product: { title: `Product variant media validation other ${suffix}`, status: 'DRAFT' },
  });
  const otherProduct = readCreatedProduct(createOtherProduct);
  const otherProductId = readId(otherProduct, 'other product');
  const otherVariantId = readFirstVariantId(otherProduct);
  productIds.push(otherProductId);

  const createBaseReadyMedia = await runStep('base ready media setup', productCreateMediaDocument, {
    productId: baseProductId,
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: 'https://placehold.co/640x480/png?text=variant-media-ready-base',
        alt: 'Variant media validation ready base',
      },
    ],
  });
  const baseReadyMediaId = readCreatedMediaId(createBaseReadyMedia, 'base ready media');
  const baseReadyMediaRead = await waitForReadyMedia(baseProductId, 1);
  const settleBaseReadyMedia = await runStep('settle base ready media', productUpdateMediaDocument, {
    productId: baseProductId,
    media: [],
  });

  const createOtherReadyMedia = await runStep('other ready media setup', productCreateMediaDocument, {
    productId: otherProductId,
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: 'https://placehold.co/640x480/png?text=variant-media-ready-other',
        alt: 'Variant media validation ready other',
      },
    ],
  });
  const otherReadyMediaId = readCreatedMediaId(createOtherReadyMedia, 'other ready media');
  const otherReadyMediaRead = await waitForReadyMedia(otherProductId, 1);
  const settleOtherReadyMedia = await runStep('settle other ready media', productUpdateMediaDocument, {
    productId: otherProductId,
    media: [],
  });

  const createBaseProcessingMedia = await runStep('base processing media setup', productCreateMediaDocument, {
    productId: baseProductId,
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: 'https://placehold.co/640x480/png?text=variant-media-processing-base',
        alt: 'Variant media validation processing base',
      },
    ],
  });
  const baseProcessingMediaId = readCreatedMediaId(createBaseProcessingMedia, 'base processing media');

  const appendVariantFromOtherProduct = await runStep('append variant from other product', appendDocument, {
    productId: baseProductId,
    variantMedia: [{ variantId: otherVariantId, mediaIds: [baseReadyMediaId] }],
  });
  assertUserErrors(appendVariantFromOtherProduct, 'productVariantAppendMedia', 'append variant from other product');

  const appendMediaFromOtherProduct = await runStep('append media from other product', appendDocument, {
    productId: baseProductId,
    variantMedia: [{ variantId: baseVariantId, mediaIds: [otherReadyMediaId] }],
  });
  assertUserErrors(appendMediaFromOtherProduct, 'productVariantAppendMedia', 'append media from other product');

  const appendProcessingMedia = await runStep('append processing media', appendDocument, {
    productId: baseProductId,
    variantMedia: [{ variantId: baseVariantId, mediaIds: [baseProcessingMediaId] }],
  });
  assertUserErrors(appendProcessingMedia, 'productVariantAppendMedia', 'append processing media');

  const detachUnattachedMedia = await runStep('detach unattached media', detachDocument, {
    productId: baseProductId,
    variantMedia: [{ variantId: baseVariantId, mediaIds: [baseReadyMediaId] }],
  });
  assertUserErrors(detachUnattachedMedia, 'productVariantDetachMedia', 'detach unattached media');

  fixturePayload = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes: [
      'Validation branches for productVariantAppendMedia and productVariantDetachMedia.',
      'The script creates disposable products and media, waits only for the ready-media branches, and deletes the products during cleanup.',
    ],
    operations: {
      createBaseProduct,
      createOtherProduct,
      createBaseReadyMedia,
      baseReadyMediaRead,
      settleBaseReadyMedia,
      createOtherReadyMedia,
      otherReadyMediaRead,
      settleOtherReadyMedia,
      createBaseProcessingMedia,
      appendVariantFromOtherProduct,
      appendMediaFromOtherProduct,
      appendProcessingMedia,
      detachUnattachedMedia,
    },
    upstreamCalls: [],
    cleanup,
  };
} finally {
  for (const productId of productIds.reverse()) {
    cleanup.push(await runRaw('cleanup productDelete', productDeleteMutation, { input: { id: productId } }));
  }
}

if (!fixturePayload) {
  throw new Error('Product variant media validation capture did not produce a fixture payload.');
}

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixturePayload, null, 2)}\n`, 'utf8');
console.log(`Wrote product variant media validation conformance fixture to ${outputPath}`);
