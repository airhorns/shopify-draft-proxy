/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedInteraction = {
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
  status: number;
  response: unknown;
};

type BulkOperationNode = {
  id?: unknown;
  status?: unknown;
  objectCount?: unknown;
  rootObjectCount?: unknown;
  url?: unknown;
};

const scenarioId = 'bulk-operation-root-count-and-search-filters';
const terminalStatuses = new Set(['CANCELED', 'COMPLETED', 'EXPIRED', 'FAILED']);
const pollIntervalMs = readPositiveIntegerEnv('SHOPIFY_CONFORMANCE_BULK_POLL_INTERVAL_MS', 1_500);
const maxPolls = readPositiveIntegerEnv('SHOPIFY_CONFORMANCE_BULK_MAX_POLLS', 60);
const configEnv = {
  ...process.env,
  SHOPIFY_CONFORMANCE_API_VERSION: process.env['SHOPIFY_CONFORMANCE_BULK_API_VERSION'] ?? '2026-04',
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  env: configEnv,
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'bulk-operations');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const bulkOperationFields = `
  id
  status
  type
  errorCode
  createdAt
  completedAt
  objectCount
  rootObjectCount
  fileSize
  url
  partialDataUrl
  query
`;

const productCreateMutation = `mutation BulkOperationRootCountProductCreate($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      title
      tags
      metafields(first: 5) {
        nodes {
          id
          namespace
          key
          value
        }
      }
      variants(first: 5) {
        nodes {
          id
          title
          sku
        }
      }
    }
    userErrors {
      field
      message
    }
  }
}`;

const productCreateMediaMutation = `mutation BulkOperationRootCountProductCreateMedia($productId: ID!, $media: [CreateMediaInput!]!) {
  productCreateMedia(productId: $productId, media: $media) {
    media {
      id
      alt
      mediaContentType
      status
    }
    mediaUserErrors {
      field
      message
    }
  }
}`;

const productVariantsBulkUpdateMutation = `mutation BulkOperationRootCountProductVariantUpdate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
  productVariantsBulkUpdate(productId: $productId, variants: $variants) {
    productVariants {
      id
      sku
      metafields(first: 5, namespace: "custom") {
        nodes {
          id
          namespace
          key
          value
        }
      }
    }
    userErrors {
      field
      message
    }
  }
}`;

const productVariantAppendMediaMutation = `mutation BulkOperationRootCountProductVariantAppendMedia($productId: ID!, $variantMedia: [ProductVariantAppendMediaInput!]!) {
  productVariantAppendMedia(productId: $productId, variantMedia: $variantMedia) {
    productVariants {
      id
      media(first: 5) {
        nodes {
          id
          alt
          mediaContentType
        }
      }
    }
    userErrors {
      field
      message
    }
  }
}`;

const productDeleteMutation = `mutation BulkOperationRootCountProductDelete($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}`;

const productChildConnectionsQuery = `query BulkOperationRootCountProductChildrenVisible($id: ID!, $namespace: String!) {
  product(id: $id) {
    id
    media(first: 5) {
      nodes {
        id
        alt
        mediaContentType
        status
      }
    }
    metafields(first: 5, namespace: $namespace) {
      nodes {
        id
        namespace
        key
        value
      }
    }
  }
}`;

const productVariantChildConnectionsQuery = `query BulkOperationRootCountProductVariantChildrenVisible($id: ID!, $namespace: String!) {
  productVariant(id: $id) {
    id
    sku
    media(first: 5) {
      nodes {
        id
        alt
        mediaContentType
      }
    }
    metafields(first: 5, namespace: $namespace) {
      nodes {
        id
        namespace
        key
        value
      }
    }
  }
}`;

const productSearchQuery = `query BulkOperationRootCountProductVisible($query: String!) {
  products(first: 1, query: $query) {
    nodes {
      id
      title
      tags
    }
  }
}`;

const bulkOperationRunQueryMutation = `mutation BulkOperationRootCountRunQuery($query: String!) {
  bulkOperationRunQuery(query: $query) {
    bulkOperation {
      ${bulkOperationFields}
    }
    userErrors {
      field
      message
      code
    }
  }
}`;

const bulkOperationByIdQuery = `query BulkOperationByIdCapture($id: ID!) {
  bulkOperation(id: $id) {
    ${bulkOperationFields}
  }
}`;

const completedAfterFilterQuery = `query BulkOperationsCompletedAfterFilter {
  bulkOperations(first: 1, query: "status:completed AND created_at:>=2000-01-01") {
    nodes {
      id
      status
      type
      createdAt
    }
  }
}`;

const createdBeforeFilterQuery = `query BulkOperationsCreatedBeforeFilter {
  bulkOperations(first: 5, query: "created_at:<2000-01-01") {
    nodes {
      id
    }
  }
}`;

const unknownFilterQuery = `query BulkOperationsUnknownFilterWarning {
  bulkOperations(first: 1, query: "made_up:value") {
    nodes {
      id
    }
  }
}`;

function readPositiveIntegerEnv(name: string, fallback: number): number {
  const rawValue = process.env[name];
  if (!rawValue) {
    return fallback;
  }

  const parsed = Number.parseInt(rawValue, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer when set.`);
  }

  return parsed;
}

function captureResult(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): CapturedInteraction {
  return {
    operationName,
    query,
    variables,
    status: result.status,
    response: result.payload,
  };
}

async function capture(
  operationName: string,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<CapturedInteraction> {
  const result = await runGraphqlRequest(query, variables);
  return captureResult(operationName, query, variables, result);
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readData(interaction: CapturedInteraction): Record<string, unknown> | null {
  const response = asRecord(interaction.response);
  return asRecord(response?.['data']);
}

function readPayloadUserErrors(interaction: CapturedInteraction, payloadFieldName: string): unknown[] {
  const data = readData(interaction);
  const payload = asRecord(data?.[payloadFieldName]);
  const errorFieldName = payloadFieldName === 'productCreateMedia' ? 'mediaUserErrors' : 'userErrors';
  const userErrors = payload?.[errorFieldName];
  return Array.isArray(userErrors) ? userErrors : [];
}

function readCreatedProductId(interaction: CapturedInteraction): string {
  const data = readData(interaction);
  const payload = asRecord(data?.['productCreate']);
  const product = asRecord(payload?.['product']);
  const id = product?.['id'];
  if (typeof id !== 'string') {
    throw new Error('productCreate did not return a product id.');
  }
  return id;
}

function readCreatedProductDefaultVariantId(interaction: CapturedInteraction): string {
  const data = readData(interaction);
  const payload = asRecord(data?.['productCreate']);
  const product = asRecord(payload?.['product']);
  const variants = asRecord(product?.['variants']);
  const nodes = variants?.['nodes'];
  const firstVariant = Array.isArray(nodes) ? asRecord(nodes[0]) : null;
  const id = firstVariant?.['id'];
  if (typeof id !== 'string') {
    throw new Error('productCreate did not expose a default variant id.');
  }
  return id;
}

function readCreatedProductMediaId(interaction: CapturedInteraction): string {
  const data = readData(interaction);
  const payload = asRecord(data?.['productCreateMedia']);
  const media = payload?.['media'];
  const firstMedia = Array.isArray(media) ? asRecord(media[0]) : null;
  const id = firstMedia?.['id'];
  if (typeof id !== 'string') {
    throw new Error('productCreateMedia did not return a media id.');
  }
  return id;
}

function readBulkOperationFromPayload(interaction: CapturedInteraction): BulkOperationNode | null {
  const data = readData(interaction);
  const payload = asRecord(data?.['bulkOperationRunQuery']);
  return asRecord(payload?.['bulkOperation']);
}

function readBulkOperationFromField(interaction: CapturedInteraction): BulkOperationNode | null {
  const data = readData(interaction);
  return asRecord(data?.['bulkOperation']);
}

function readBulkOperationId(node: BulkOperationNode | null): string {
  const id = node?.id;
  if (typeof id !== 'string') {
    throw new Error('bulkOperationRunQuery did not return a BulkOperation id.');
  }
  return id;
}

function readBulkOperationStatus(node: BulkOperationNode | null): string | null {
  return typeof node?.status === 'string' ? node.status : null;
}

function readBulkOperationUrl(node: BulkOperationNode | null): string | null {
  return typeof node?.url === 'string' ? node.url : null;
}

function readCount(value: unknown, fieldName: string): number {
  if (typeof value !== 'string') {
    throw new Error(`${fieldName} was not a string.`);
  }
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed)) {
    throw new Error(`${fieldName} was not numeric: ${value}`);
  }
  return parsed;
}

function assertNoUserErrors(interaction: CapturedInteraction, payloadFieldName: string): void {
  const userErrors = readPayloadUserErrors(interaction, payloadFieldName);
  if (userErrors.length > 0) {
    throw new Error(`${payloadFieldName} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

async function waitForProductSearch(tag: string, productId: string): Promise<CapturedInteraction[]> {
  const query = `tag:${tag}`;
  const probes: CapturedInteraction[] = [];
  for (let index = 0; index < maxPolls; index += 1) {
    if (index > 0) {
      await sleep(pollIntervalMs);
    }
    const probe = await capture('BulkOperationRootCountProductVisible', productSearchQuery, { query });
    probes.push(probe);
    const nodes = asRecord(readData(probe)?.['products'])?.['nodes'];
    if (Array.isArray(nodes) && nodes.some((node) => asRecord(node)?.['id'] === productId)) {
      return probes;
    }
  }
  throw new Error(`Created product ${productId} was not visible via products(query: ${query}).`);
}

async function waitForProductChildConnections(
  productId: string,
  namespace: string,
  mediaAlt: string,
  metafieldKey: string,
): Promise<CapturedInteraction[]> {
  const probes: CapturedInteraction[] = [];
  for (let index = 0; index < maxPolls; index += 1) {
    if (index > 0) {
      await sleep(pollIntervalMs);
    }
    const probe = await capture('BulkOperationRootCountProductChildrenVisible', productChildConnectionsQuery, {
      id: productId,
      namespace,
    });
    probes.push(probe);
    const product = asRecord(readData(probe)?.['product']);
    const mediaNodes = asRecord(product?.['media'])?.['nodes'];
    const metafieldNodes = asRecord(product?.['metafields'])?.['nodes'];
    const mediaVisible = Array.isArray(mediaNodes) && mediaNodes.some((node) => asRecord(node)?.['alt'] === mediaAlt);
    const metafieldVisible =
      Array.isArray(metafieldNodes) && metafieldNodes.some((node) => asRecord(node)?.['key'] === metafieldKey);
    if (mediaVisible && metafieldVisible) {
      return probes;
    }
  }
  throw new Error(`Product ${productId} did not expose expected media/metafield child connections.`);
}

async function waitForProductMediaReady(productId: string, mediaId: string): Promise<CapturedInteraction[]> {
  const probes: CapturedInteraction[] = [];
  for (let index = 0; index < maxPolls; index += 1) {
    if (index > 0) {
      await sleep(pollIntervalMs);
    }
    const probe = await capture('BulkOperationRootCountProductMediaReady', productChildConnectionsQuery, {
      id: productId,
      namespace: metafieldNamespace,
    });
    probes.push(probe);
    const product = asRecord(readData(probe)?.['product']);
    const mediaNodes = asRecord(product?.['media'])?.['nodes'];
    const mediaReady =
      Array.isArray(mediaNodes) &&
      mediaNodes.some((node) => asRecord(node)?.['id'] === mediaId && asRecord(node)?.['status'] === 'READY');
    if (mediaReady) {
      return probes;
    }
  }
  throw new Error(`Product media ${mediaId} was not READY on product ${productId}.`);
}

async function waitForProductVariantChildConnections(
  variantId: string,
  namespace: string,
  mediaAlt: string,
  metafieldKey: string,
): Promise<CapturedInteraction[]> {
  const probes: CapturedInteraction[] = [];
  for (let index = 0; index < maxPolls; index += 1) {
    if (index > 0) {
      await sleep(pollIntervalMs);
    }
    const probe = await capture(
      'BulkOperationRootCountProductVariantChildrenVisible',
      productVariantChildConnectionsQuery,
      {
        id: variantId,
        namespace,
      },
    );
    probes.push(probe);
    const variant = asRecord(readData(probe)?.['productVariant']);
    const mediaNodes = asRecord(variant?.['media'])?.['nodes'];
    const metafieldNodes = asRecord(variant?.['metafields'])?.['nodes'];
    const mediaVisible = Array.isArray(mediaNodes) && mediaNodes.some((node) => asRecord(node)?.['alt'] === mediaAlt);
    const metafieldVisible =
      Array.isArray(metafieldNodes) && metafieldNodes.some((node) => asRecord(node)?.['key'] === metafieldKey);
    if (mediaVisible && metafieldVisible) {
      return probes;
    }
  }
  throw new Error(`Product variant ${variantId} did not expose expected media/metafield child connections.`);
}

async function pollBulkOperationToTerminal(id: string): Promise<CapturedInteraction[]> {
  const polls: CapturedInteraction[] = [];
  for (let index = 0; index < maxPolls; index += 1) {
    if (index > 0) {
      await sleep(pollIntervalMs);
    }
    const poll = await capture(`BulkOperationRootCountStatusPoll${index + 1}`, bulkOperationByIdQuery, { id });
    polls.push(poll);
    const status = readBulkOperationStatus(readBulkOperationFromField(poll));
    if (status !== null && terminalStatuses.has(status)) {
      return polls;
    }
  }
  throw new Error(`BulkOperation ${id} did not reach a terminal status.`);
}

function findTerminalBulkOperation(polls: CapturedInteraction[]): BulkOperationNode {
  const terminal = polls
    .map((poll) => readBulkOperationFromField(poll))
    .find((operation) => terminalStatuses.has(readBulkOperationStatus(operation) ?? ''));
  if (!terminal) {
    throw new Error('No terminal BulkOperation found in status polls.');
  }
  return terminal;
}

async function captureBulkOperationResult(url: string): Promise<Record<string, unknown>> {
  const response = await fetch(url);
  const text = await response.text();
  const records = text
    .trim()
    .split('\n')
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line) as unknown);

  return {
    status: response.status,
    contentType: response.headers.get('content-type'),
    byteLength: Buffer.byteLength(text, 'utf8'),
    body: text,
    records,
  };
}

const runId = `bulk-root-${Date.now().toString(36)}-${process.pid.toString(36)}`;
const tag = `conformance-${runId}`;
const metafieldNamespace = 'custom';
const metafieldKey = `bulk_child_${Date.now().toString(36)}_${process.pid.toString(36)}`;
const metafieldValue = `bulk child ${runId}`;
const mediaAlt = `Bulk child media ${runId}`;
const variantSku = `bulk-variant-${runId}`;
const variantMetafieldKey = `bulk_variant_child_${Date.now().toString(36)}_${process.pid.toString(36)}`;
const variantMetafieldValue = `bulk variant child ${runId}`;
const productVariables = {
  product: {
    title: `Bulk root count ${runId}`,
    tags: ['conformance', 'bulk-root-count', tag],
    metafields: [
      {
        namespace: metafieldNamespace,
        key: metafieldKey,
        type: 'single_line_text_field',
        value: metafieldValue,
      },
    ],
  },
};

function buildVariantUpdateVariables(productId: string, variantId: string): Record<string, unknown> {
  return {
    productId,
    variants: [
      {
        id: variantId,
        inventoryItem: {
          sku: variantSku,
        },
        metafields: [
          {
            namespace: metafieldNamespace,
            key: variantMetafieldKey,
            type: 'single_line_text_field',
            value: variantMetafieldValue,
          },
        ],
      },
    ],
  };
}

function buildVariantAppendMediaVariables(
  productId: string,
  variantId: string,
  mediaId: string,
): Record<string, unknown> {
  return {
    productId,
    variantMedia: [
      {
        variantId,
        mediaIds: [mediaId],
      },
    ],
  };
}

const bulkQuery = `#graphql
{
  products(query: "tag:${tag}") {
    edges {
      node {
        id
        title
        variants {
          edges {
            node {
              id
              title
              sku
            }
          }
        }
        media {
          edges {
            node {
              id
              alt
              mediaContentType
            }
          }
        }
        metafields(namespace: "${metafieldNamespace}") {
          edges {
            node {
              id
              namespace
              key
              value
            }
          }
        }
      }
    }
  }
}`;
const bulkVariantQuery = `#graphql
{
  productVariants(query: "sku:${variantSku}") {
    edges {
      node {
        id
        sku
        media {
          edges {
            node {
              id
              alt
              mediaContentType
            }
          }
        }
        metafields(namespace: "${metafieldNamespace}") {
          edges {
            node {
              id
              namespace
              key
              value
            }
          }
        }
      }
    }
  }
}`;

// Keep these byte-identical to the production hydration documents. The parity
// cassette matches the exact GraphQL text and variables sent by the proxy.
const productCatalogHydrationQuery =
  'query BulkProductsCatalogHydrate($first: Int!, $after: String, $nestedFirst: Int!, $nestedAfter: String) { products(first: $first, after: $after) { nodes { id title bulkNested0: variants(first: $nestedFirst, after: $nestedAfter) { nodes { id title sku } pageInfo { hasNextPage endCursor } } bulkNested1: media(first: $nestedFirst, after: $nestedAfter) { nodes { id alt mediaContentType } pageInfo { hasNextPage endCursor } } bulkNested2: metafields(namespace: "custom", first: $nestedFirst, after: $nestedAfter) { nodes { id namespace key value } pageInfo { hasNextPage endCursor } } tags } pageInfo { hasNextPage endCursor } } }';
const productVariantCatalogHydrationQuery =
  'query BulkProductVariantsCatalogHydrate($first: Int!, $after: String, $nestedFirst: Int!, $nestedAfter: String) { productVariants(first: $first, after: $after) { nodes { id sku bulkNested0: media(first: $nestedFirst, after: $nestedAfter) { nodes { id alt mediaContentType } pageInfo { hasNextPage endCursor } } bulkNested1: metafields(namespace: "custom", first: $nestedFirst, after: $nestedAfter) { nodes { id namespace key value } pageInfo { hasNextPage endCursor } } product { id } } pageInfo { hasNextPage endCursor } } }';

async function captureCatalogPage(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedInteraction> {
  for (let attempt = 0; attempt < maxPolls; attempt += 1) {
    const page = await capture(operationName, query, variables);
    const response = asRecord(page.response);
    const errors = response?.['errors'];
    const throttled =
      Array.isArray(errors) &&
      errors.some((error) => asRecord(asRecord(error)?.['extensions'])?.['code'] === 'THROTTLED');
    if (!throttled) return page;

    const cost = asRecord(asRecord(response?.['extensions'])?.['cost']);
    const throttleStatus = asRecord(cost?.['throttleStatus']);
    const requested = typeof cost?.['requestedQueryCost'] === 'number' ? cost['requestedQueryCost'] : 350;
    const available =
      typeof throttleStatus?.['currentlyAvailable'] === 'number' ? throttleStatus['currentlyAvailable'] : 0;
    const restoreRate = typeof throttleStatus?.['restoreRate'] === 'number' ? throttleStatus['restoreRate'] : 100;
    const retryDelayMs = Math.max(1_000, Math.ceil(((requested - available) / restoreRate) * 1_000) + 250);
    await sleep(retryDelayMs);
  }

  throw new Error(`${operationName} remained throttled after ${maxPolls} attempts.`);
}

async function captureCatalogPages(
  operationName: string,
  query: string,
  rootName: 'products' | 'productVariants',
): Promise<CapturedInteraction[]> {
  const pages: CapturedInteraction[] = [];
  const seenCursors = new Set<string>();
  let after: string | null = null;

  for (let index = 0; index < 10_000; index += 1) {
    const page = await captureCatalogPage(operationName, query, {
      first: 250,
      after,
      nestedFirst: 250,
      nestedAfter: null,
    });
    pages.push(page);
    const response = asRecord(page.response);
    const errors = response?.['errors'];
    const connection = asRecord(readData(page)?.[rootName]);
    const nodes = connection?.['nodes'];
    const pageInfo = asRecord(connection?.['pageInfo']);
    if (
      page.status >= 400 ||
      (Array.isArray(errors) && errors.length > 0) ||
      !Array.isArray(nodes) ||
      typeof pageInfo?.['hasNextPage'] !== 'boolean'
    ) {
      throw new Error(`${rootName} catalog hydration page was malformed: ${JSON.stringify(page.response)}`);
    }
    for (const node of nodes) {
      const record = asRecord(node);
      for (const [key, value] of Object.entries(record ?? {})) {
        if (!key.startsWith('bulkNested')) continue;
        const nestedPageInfo = asRecord(asRecord(value)?.['pageInfo']);
        if (nestedPageInfo?.['hasNextPage'] === true) {
          throw new Error(`${rootName}.${key} exceeded the captured first 250 rows; nested-page capture is required.`);
        }
      }
    }
    if (pageInfo['hasNextPage'] === false) {
      return pages;
    }
    const endCursor = pageInfo['endCursor'];
    if (typeof endCursor !== 'string' || seenCursors.has(endCursor)) {
      throw new Error(`${rootName} catalog hydration cursor did not prove progress: ${JSON.stringify(pageInfo)}`);
    }
    seenCursors.add(endCursor);
    after = endCursor;
  }

  throw new Error(`${rootName} catalog hydration exceeded 10,000 pages.`);
}

let createdProductId: string | null = null;
let cleanup: CapturedInteraction | null = null;
const productCatalogHydrationPages = await captureCatalogPages(
  'BulkProductsCatalogHydrate',
  productCatalogHydrationQuery,
  'products',
);
const productVariantCatalogHydrationPages = await captureCatalogPages(
  'BulkProductVariantsCatalogHydrate',
  productVariantCatalogHydrationQuery,
  'productVariants',
);

try {
  const productCreate = await capture('BulkOperationRootCountProductCreate', productCreateMutation, productVariables);
  assertNoUserErrors(productCreate, 'productCreate');
  createdProductId = readCreatedProductId(productCreate);
  const defaultVariantId = readCreatedProductDefaultVariantId(productCreate);

  const productCreateMedia = await capture('BulkOperationRootCountProductCreateMedia', productCreateMediaMutation, {
    productId: createdProductId,
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: `https://placehold.co/640x480/png?text=bulk-child-${runId}`,
        alt: mediaAlt,
      },
    ],
  });
  assertNoUserErrors(productCreateMedia, 'productCreateMedia');
  const productMediaId = readCreatedProductMediaId(productCreateMedia);
  const productMediaReadyProbes = await waitForProductMediaReady(createdProductId, productMediaId);
  const productMediaReady = productMediaReadyProbes[productMediaReadyProbes.length - 1];

  const variantUpdateVariables = buildVariantUpdateVariables(createdProductId, defaultVariantId);
  const productVariantUpdate = await capture(
    'BulkOperationRootCountProductVariantUpdate',
    productVariantsBulkUpdateMutation,
    variantUpdateVariables,
  );
  assertNoUserErrors(productVariantUpdate, 'productVariantsBulkUpdate');

  const variantAppendMediaVariables = buildVariantAppendMediaVariables(
    createdProductId,
    defaultVariantId,
    productMediaId,
  );
  const productVariantAppendMedia = await capture(
    'BulkOperationRootCountProductVariantAppendMedia',
    productVariantAppendMediaMutation,
    variantAppendMediaVariables,
  );
  assertNoUserErrors(productVariantAppendMedia, 'productVariantAppendMedia');

  const productSearchProbes = await waitForProductSearch(tag, createdProductId);
  const productChildProbes = await waitForProductChildConnections(
    createdProductId,
    metafieldNamespace,
    mediaAlt,
    metafieldKey,
  );
  const productVariantChildProbes = await waitForProductVariantChildConnections(
    defaultVariantId,
    metafieldNamespace,
    mediaAlt,
    variantMetafieldKey,
  );

  const run = await capture('BulkOperationRootCountRunQuery', bulkOperationRunQueryMutation, { query: bulkQuery });
  assertNoUserErrors(run, 'bulkOperationRunQuery');
  const operationId = readBulkOperationId(readBulkOperationFromPayload(run));
  const statusPolls = await pollBulkOperationToTerminal(operationId);
  const terminalOperation = findTerminalBulkOperation(statusPolls);
  const objectCount = readCount(terminalOperation.objectCount, 'objectCount');
  const rootObjectCount = readCount(terminalOperation.rootObjectCount, 'rootObjectCount');
  if (objectCount <= rootObjectCount) {
    throw new Error(
      `Expected nested product bulk query to report objectCount > rootObjectCount, got ${objectCount} and ${rootObjectCount}.`,
    );
  }
  if (rootObjectCount !== 1) {
    throw new Error(`Expected the tagged product query to report exactly one root object, got ${rootObjectCount}.`);
  }

  const resultUrl = readBulkOperationUrl(terminalOperation);
  const result = resultUrl ? await captureBulkOperationResult(resultUrl) : null;
  const variantRun = await capture('BulkOperationRootCountVariantRunQuery', bulkOperationRunQueryMutation, {
    query: bulkVariantQuery,
  });
  assertNoUserErrors(variantRun, 'bulkOperationRunQuery');
  const variantOperationId = readBulkOperationId(readBulkOperationFromPayload(variantRun));
  const variantStatusPolls = await pollBulkOperationToTerminal(variantOperationId);
  const variantTerminalOperation = findTerminalBulkOperation(variantStatusPolls);
  const variantObjectCount = readCount(variantTerminalOperation.objectCount, 'objectCount');
  const variantRootObjectCount = readCount(variantTerminalOperation.rootObjectCount, 'rootObjectCount');
  if (variantObjectCount <= variantRootObjectCount) {
    throw new Error(
      `Expected nested product variant bulk query to report objectCount > rootObjectCount, got ${variantObjectCount} and ${variantRootObjectCount}.`,
    );
  }
  if (variantRootObjectCount !== 1) {
    throw new Error(
      `Expected the SKU-filtered variant query to report exactly one root object, got ${variantRootObjectCount}.`,
    );
  }

  const variantResultUrl = readBulkOperationUrl(variantTerminalOperation);
  const variantResult = variantResultUrl ? await captureBulkOperationResult(variantResultUrl) : null;
  const completedAfter = await capture('BulkOperationsCompletedAfterFilter', completedAfterFilterQuery);
  const createdBefore = await capture('BulkOperationsCreatedBeforeFilter', createdBeforeFilterQuery);
  const unknown = await capture('BulkOperationsUnknownFilterWarning', unknownFilterQuery);

  if (createdProductId !== null) {
    cleanup = await capture('BulkOperationRootCountProductDelete', productDeleteMutation, {
      input: { id: createdProductId },
    });
  }

  const fixture: Record<string, unknown> = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    pollConfig: {
      pollIntervalMs,
      maxPolls,
    },
    setup: {
      productCreate,
      productCreateMedia,
      productMediaReadyProbes,
      productMediaReady,
      productVariantUpdate,
      productVariantAppendMedia,
      productSearchProbes,
      productChildProbes,
      productVariantChildProbes,
    },
    run: {
      variables: { query: bulkQuery },
      response: run.response,
      interaction: run,
      statusPolls,
      terminalOperation,
      result,
    },
    variantRun: {
      variables: { query: bulkVariantQuery },
      response: variantRun.response,
      interaction: variantRun,
      statusPolls: variantStatusPolls,
      terminalOperation: variantTerminalOperation,
      result: variantResult,
    },
    filters: {
      completedAfter,
      createdBefore,
      unknown,
    },
    upstreamCalls: [...productCatalogHydrationPages, ...productVariantCatalogHydrationPages],
    cleanup,
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (createdProductId !== null && cleanup === null) {
    await capture('BulkOperationRootCountProductDelete', productDeleteMutation, {
      input: { id: createdProductId },
    });
  }
}
