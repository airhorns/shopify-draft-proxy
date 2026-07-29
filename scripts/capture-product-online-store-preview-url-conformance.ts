/* oxlint-disable no-console -- CLI recorder intentionally writes capture status to stdout. */
import 'dotenv/config';

import path from 'node:path';

import {
  createAdminGraphqlClient,
  type AdminGraphqlClient,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';
import { readArray, readRecord, requireString, type JsonRecord } from './conformance-capture-lib.js';
import { mkdir, readFile, writeFile } from 'node:fs/promises';

const scenarioId = 'product-online-store-preview-url-contract';
const secondaryApiVersion = '2026-07';
const requestPath = 'config/parity-requests/products/product-online-store-preview-url-contract.graphql';
const fixtureName = `${scenarioId}.json`;
const specPath = `config/parity-specs/products/${scenarioId}.json`;

type CaptureEntry = {
  query: string;
  variables: JsonRecord;
  response: {
    status: number;
    payload: JsonRecord;
  };
};

const createMutation = `mutation ProductOnlineStorePreviewUrlCreate($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      status
      publishedAt
      onlineStorePreviewUrl
    }
    userErrors {
      field
      message
    }
  }
}`;

const changeStatusMutation = `mutation ProductOnlineStorePreviewUrlArchive($productId: ID!, $status: ProductStatus!) {
  productChangeStatus(productId: $productId, status: $status) {
    product {
      id
      status
    }
    userErrors {
      field
      message
    }
  }
}`;

const deleteMutation = `mutation ProductOnlineStorePreviewUrlDelete($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}`;

function assertTransportSuccess(entry: CaptureEntry, label: string): void {
  if (entry.response.status < 200 || entry.response.status >= 300) {
    throw new Error(`${label} returned HTTP ${entry.response.status}: ${JSON.stringify(entry.response.payload)}`);
  }
}

function dataObject(entry: CaptureEntry, label: string): JsonRecord {
  const data = readRecord(entry.response.payload['data']);
  if (!data) {
    throw new Error(`${label} did not return a data object: ${JSON.stringify(entry.response.payload)}`);
  }
  return data;
}

function mutationRoot(entry: CaptureEntry, rootName: string, label: string): JsonRecord {
  assertTransportSuccess(entry, label);
  if (entry.response.payload['errors'] !== undefined) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(entry.response.payload['errors'])}`);
  }
  const root = readRecord(dataObject(entry, label)[rootName]);
  if (!root) {
    throw new Error(`${label} did not return ${rootName}: ${JSON.stringify(entry.response.payload)}`);
  }
  const userErrors = readArray(root['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
  return root;
}

function assertShopifyProductPreviewUrl(value: unknown, label: string): void {
  const rawUrl = requireString(value, label);
  const previewUrl = new URL(rawUrl);
  if (
    previewUrl.protocol !== 'https:' ||
    !previewUrl.hostname.endsWith('.shopifypreview.com') ||
    previewUrl.pathname !== '/products_preview' ||
    !previewUrl.searchParams.get('preview_key') ||
    !previewUrl.searchParams.get('_bt')
  ) {
    throw new Error(`${label} was not a signed Shopify product preview URL.`);
  }
}

function assertExtantProductPreview(entry: CaptureEntry, expectedStatus: string, label: string): void {
  assertTransportSuccess(entry, label);
  if (entry.response.payload['errors'] !== undefined) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(entry.response.payload['errors'])}`);
  }
  const product = readRecord(dataObject(entry, label)['product']);
  if (!product || product['status'] !== expectedStatus) {
    throw new Error(`${label} did not return an extant ${expectedStatus} product.`);
  }
  assertShopifyProductPreviewUrl(product['onlineStorePreviewUrl'], `${label}.onlineStorePreviewUrl`);
}

function assertDeletedProductNull(entry: CaptureEntry, label: string): void {
  assertTransportSuccess(entry, label);
  if (entry.response.payload['errors'] !== undefined || dataObject(entry, label)['product'] !== null) {
    throw new Error(`${label} did not return product: null: ${JSON.stringify(entry.response.payload)}`);
  }
}

function assertMalformedIdError(entry: CaptureEntry): void {
  assertTransportSuccess(entry, 'malformed product id read');
  const errors = readArray(entry.response.payload['errors']);
  if (errors.length === 0) {
    throw new Error(`Malformed product id did not return a GraphQL error: ${JSON.stringify(entry.response.payload)}`);
  }
}

async function captureEntry(client: AdminGraphqlClient, query: string, variables: JsonRecord): Promise<CaptureEntry> {
  const response: ConformanceGraphqlResult<JsonRecord> = await client.runGraphqlRequest<JsonRecord>(query, variables);
  return {
    query,
    variables,
    response: {
      status: response.status,
      payload: response.payload as JsonRecord,
    },
  };
}

function recordedUpstreamCall(entry: CaptureEntry, apiVersion: string): JsonRecord {
  return {
    method: 'POST',
    apiSurface: 'admin',
    apiVersion,
    path: `/admin/api/${apiVersion}/graphql.json`,
    operationName: 'ProductOnlineStorePreviewUrlContract',
    query: entry.query,
    variables: entry.variables,
    response: {
      status: entry.response.status,
      body: entry.response.payload,
    },
  };
}

function buildSpec(apiVersion: string, fixturePath: string): JsonRecord {
  return {
    scenarioId,
    operationNames: ['product'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'nullability-parity', 'error-parity', 'version-behavior'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/product_set_hydration.rs', 'tests/graphql_routes/store_state.rs'],
    proxyRequest: {
      documentPath: requestPath,
      apiVersion,
      variablesCapturePath: '$.draftRead.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'extant-draft-product-authoritative-preview-url',
          capturePath: '$.draftRead.response.payload',
          proxyPath: '$',
        },
        {
          name: 'extant-archived-product-authoritative-preview-url',
          capturePath: '$.archivedRead.response.payload',
          proxyRequest: {
            documentPath: requestPath,
            apiVersion,
            variablesCapturePath: '$.archivedRead.variables',
          },
          proxyPath: '$',
        },
        {
          name: 'deleted-product-null-boundary',
          capturePath: '$.deletedRead.response.payload',
          proxyRequest: {
            documentPath: requestPath,
            apiVersion,
            variablesCapturePath: '$.deletedRead.variables',
          },
          proxyPath: '$',
        },
        {
          name: 'malformed-product-id-error-boundary',
          capturePath: '$.malformedIdRead.response.payload.errors',
          proxyRequest: {
            documentPath: requestPath,
            apiVersion,
            variablesCapturePath: '$.malformedIdRead.variables',
          },
          proxyPath: '$.errors',
        },
      ],
    },
    notes:
      'Strict live parity for Product.onlineStorePreviewUrl. The configured Online Store shop returns signed Shopify preview URLs for extant DRAFT and ARCHIVED products, product: null after deletion, and a GraphQL error for a malformed global id. The same extant/deleted observations are also recorded on Admin API 2026-07 to make the version context explicit.',
  };
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const headers = buildAdminAuthHeaders(adminAccessToken);
const primaryClient = createAdminGraphqlClient({ adminOrigin, apiVersion, headers });
const secondaryClient = createAdminGraphqlClient({ adminOrigin, apiVersion: secondaryApiVersion, headers });
const readQuery = await readFile(requestPath, 'utf8');
const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

const createVariables = {
  product: {
    title: `Hermes Preview URL Contract ${stamp}`,
    status: 'DRAFT',
  },
};
const malformedIdVariables = { id: 'not-a-gid' };
let productId: string | null = null;
let deleted = false;
let createEntry: CaptureEntry | null = null;
let draftRead: CaptureEntry | null = null;
let secondaryDraftRead: CaptureEntry | null = null;
let archiveEntry: CaptureEntry | null = null;
let archivedRead: CaptureEntry | null = null;
let secondaryArchivedRead: CaptureEntry | null = null;
let deleteEntry: CaptureEntry | null = null;
let deletedRead: CaptureEntry | null = null;
let secondaryDeletedRead: CaptureEntry | null = null;
let malformedIdRead: CaptureEntry | null = null;

try {
  createEntry = await captureEntry(primaryClient, createMutation, createVariables);
  const createRoot = mutationRoot(createEntry, 'productCreate', 'preview contract productCreate');
  const createdProduct = readRecord(createRoot['product']);
  productId = requireString(createdProduct?.['id'], 'productCreate.product.id');

  const productVariables = { id: productId };
  draftRead = await captureEntry(primaryClient, readQuery, productVariables);
  assertExtantProductPreview(draftRead, 'DRAFT', `${apiVersion} draft read`);
  secondaryDraftRead = await captureEntry(secondaryClient, readQuery, productVariables);
  assertExtantProductPreview(secondaryDraftRead, 'DRAFT', `${secondaryApiVersion} draft read`);

  archiveEntry = await captureEntry(primaryClient, changeStatusMutation, {
    productId,
    status: 'ARCHIVED',
  });
  mutationRoot(archiveEntry, 'productChangeStatus', 'preview contract productChangeStatus');

  archivedRead = await captureEntry(primaryClient, readQuery, productVariables);
  assertExtantProductPreview(archivedRead, 'ARCHIVED', `${apiVersion} archived read`);
  secondaryArchivedRead = await captureEntry(secondaryClient, readQuery, productVariables);
  assertExtantProductPreview(secondaryArchivedRead, 'ARCHIVED', `${secondaryApiVersion} archived read`);

  deleteEntry = await captureEntry(primaryClient, deleteMutation, { input: { id: productId } });
  const deleteRoot = mutationRoot(deleteEntry, 'productDelete', 'preview contract productDelete');
  if (deleteRoot['deletedProductId'] !== productId) {
    throw new Error('productDelete did not return the created product id.');
  }
  deleted = true;

  deletedRead = await captureEntry(primaryClient, readQuery, productVariables);
  assertDeletedProductNull(deletedRead, `${apiVersion} deleted read`);
  secondaryDeletedRead = await captureEntry(secondaryClient, readQuery, productVariables);
  assertDeletedProductNull(secondaryDeletedRead, `${secondaryApiVersion} deleted read`);

  malformedIdRead = await captureEntry(primaryClient, readQuery, malformedIdVariables);
  assertMalformedIdError(malformedIdRead);
} finally {
  if (productId !== null && !deleted) {
    try {
      await primaryClient.runGraphqlRequest(deleteMutation, { input: { id: productId } });
    } catch (error) {
      console.error(`Best-effort preview contract product cleanup failed: ${String(error)}`);
    }
  }
}

if (
  !createEntry ||
  !draftRead ||
  !secondaryDraftRead ||
  !archiveEntry ||
  !archivedRead ||
  !secondaryArchivedRead ||
  !deleteEntry ||
  !deletedRead ||
  !secondaryDeletedRead ||
  !malformedIdRead
) {
  throw new Error('Preview URL contract capture did not complete every required branch.');
}

const fixturePath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products', fixtureName);
const fixture = {
  scenarioId,
  capturedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  storeDomain,
  apiVersion,
  secondaryApiVersion,
  liveGatewaySideEffects: true,
  notes: [
    'The configured store has Online Store preview context: extant DRAFT and ARCHIVED products returned signed shopifypreview.com URLs on both recorded Admin API versions.',
    'Deleting the product changed the same product(id:) request to product: null on both recorded Admin API versions.',
    'A malformed global id was rejected as a GraphQL variable error before product resolution.',
    'The signed URL is authoritative only for the observed store/resource response; its host, preview_key, and signed _bt value are opaque capture data and are never production defaults.',
  ],
  create: createEntry,
  draftRead,
  secondaryDraftRead,
  archive: archiveEntry,
  archivedRead,
  secondaryArchivedRead,
  delete: deleteEntry,
  deletedRead,
  secondaryDeletedRead,
  malformedIdRead,
  upstreamCalls: [
    recordedUpstreamCall(draftRead, apiVersion),
    recordedUpstreamCall(archivedRead, apiVersion),
    recordedUpstreamCall(deletedRead, apiVersion),
  ],
};

await mkdir(path.dirname(fixturePath), { recursive: true });
await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
await mkdir(path.dirname(specPath), { recursive: true });
await writeFile(specPath, `${JSON.stringify(buildSpec(apiVersion, fixturePath), null, 2)}\n`, 'utf8');

console.log(JSON.stringify({ ok: true, scenarioId, fixturePath, specPath }, null, 2));
