/* oxlint-disable no-console -- CLI capture scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
if (apiVersion !== '2026-04') {
  throw new Error(`inventory cold-read capture requires SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${apiVersion}`);
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'inventory-live-hybrid-cold-authoritative-reads.json');
const parityRequestPath = path.join(
  'config',
  'parity-requests',
  'products',
  'inventory-live-hybrid-cold-authoritative-reads.graphql',
);
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateMutation = `#graphql
  mutation InventoryColdReadsProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        variants(first: 1) {
          nodes {
            id
            sku
            inventoryItem {
              id
              inventoryLevels(first: 1) {
                nodes { id }
              }
            }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const coldReadsQuery = await readFile(parityRequestPath, 'utf8');

const productDeleteMutation = `#graphql
  mutation InventoryColdReadsProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

async function runGraphql(query: string, variables: Record<string, unknown>): Promise<JsonRecord> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(JSON.stringify(result, null, 2));
  }
  return result.payload as JsonRecord;
}

function readRecord(value: unknown, label: string): JsonRecord {
  if (typeof value === 'object' && value !== null && !Array.isArray(value)) return value as JsonRecord;
  throw new Error(`${label} was not an object: ${JSON.stringify(value)}`);
}

function readPath(value: unknown, segments: Array<string | number>, label: string): unknown {
  let current = value;
  for (const segment of segments) {
    if (typeof segment === 'number') {
      if (!Array.isArray(current) || current[segment] === undefined) {
        throw new Error(`${label} was missing index ${segment}: ${JSON.stringify(value)}`);
      }
      current = current[segment];
    } else {
      current = readRecord(current, label)[segment];
    }
  }
  return current;
}

function readString(value: unknown, segments: Array<string | number>, label: string): string {
  const candidate = readPath(value, segments, label);
  if (typeof candidate === 'string' && candidate.length > 0) return candidate;
  throw new Error(`${label} was missing ${segments.join('.')}: ${JSON.stringify(value)}`);
}

function expectNoUserErrors(value: unknown, segments: Array<string | number>, label: string): void {
  const candidate = readPath(value, segments, label);
  if (!Array.isArray(candidate) || candidate.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(candidate, null, 2)}`);
  }
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const createVariables = {
  product: {
    title: `Inventory cold authoritative reads ${runId}`,
    status: 'ACTIVE',
  },
};
let productId: string | null = null;
let cleanup: JsonRecord | null = null;

try {
  const createResponse = await runGraphql(productCreateMutation, createVariables);
  expectNoUserErrors(createResponse, ['data', 'productCreate', 'userErrors'], 'productCreate');
  productId = readString(createResponse, ['data', 'productCreate', 'product', 'id'], 'productCreate');
  const inventoryItemId = readString(
    createResponse,
    ['data', 'productCreate', 'product', 'variants', 'nodes', 0, 'inventoryItem', 'id'],
    'productCreate inventory item',
  );
  const inventoryLevelId = readString(
    createResponse,
    ['data', 'productCreate', 'product', 'variants', 'nodes', 0, 'inventoryItem', 'inventoryLevels', 'nodes', 0, 'id'],
    'productCreate inventory level',
  );
  const coldReadVariables = {
    inventoryItemId,
    inventoryLevelId,
    itemQuery: `id:${inventoryItemId.split('/').at(-1) ?? inventoryItemId}`,
  };
  const coldReadResponse = await runGraphql(coldReadsQuery, coldReadVariables);

  const capturePayload = {
    scenarioId: 'inventory-live-hybrid-cold-authoritative-reads',
    capturedAt: new Date().toISOString(),
    source: 'live-shopify',
    storeDomain,
    apiVersion,
    summary:
      'Cold authoritative inventoryItem, inventoryItems, and inventoryLevel reads for one disposable Shopify product-backed inventory item.',
    setup: {
      productCreate: {
        query: productCreateMutation,
        variables: createVariables,
        response: createResponse,
      },
    },
    coldRead: {
      query: coldReadsQuery,
      variables: coldReadVariables,
      response: coldReadResponse,
    },
    cleanup: null as JsonRecord | null,
    upstreamCalls: [
      {
        operationName: 'ColdInventoryAuthoritativeReads',
        variables: coldReadVariables,
        query: coldReadsQuery,
        response: {
          status: 200,
          body: coldReadResponse,
        },
      },
    ],
  };

  cleanup = await runGraphql(productDeleteMutation, { input: { id: productId } });
  capturePayload.cleanup = cleanup;
  await writeFile(outputPath, `${JSON.stringify(capturePayload, null, 2)}\n`, 'utf8');
} finally {
  if (productId !== null && cleanup === null) {
    try {
      cleanup = await runGraphql(productDeleteMutation, { input: { id: productId } });
    } catch (error) {
      console.warn(`Product cleanup failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}

console.log(JSON.stringify({ ok: true, outputPath, apiVersion, storeDomain }, null, 2));
