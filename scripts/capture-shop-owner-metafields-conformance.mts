/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type Capture = {
  query: string;
  variables: JsonRecord;
  status: number;
  response: unknown;
};

const ownerMetafieldsHydrateQuery =
  'query OwnerMetafieldsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { __typename id ... on Product { id title handle status totalInventory tracksInventory createdAt updatedAt metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } } ... on ProductVariant { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } product { id title handle status totalInventory tracksInventory createdAt updatedAt } metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Collection { id title handle metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Customer { id displayName email metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Order { id name metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } ... on Company { id name metafields(first: 250) { nodes { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } } }';

const shopDocument = `#graphql
  query ShopOwnerMetafieldsShopId {
    shop {
      id
      name
      myshopifyDomain
    }
  }
`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'shop-owner-metafields-read-after-write.json');
const setDocumentPath = path.join('config', 'parity-requests', 'metafields', 'shop-owner-metafields-set.graphql');
const readDocumentPath = path.join('config', 'parity-requests', 'metafields', 'shop-owner-metafields-read.graphql');
const deleteDocumentPath = path.join('config', 'parity-requests', 'metafields', 'shop-owner-metafields-delete.graphql');

const setDocument = await readFile(setDocumentPath, 'utf8');
const readDocument = await readFile(readDocumentPath, 'utf8');
const deleteDocument = await readFile(deleteDocumentPath, 'utf8');
const runId = Date.now().toString(36);

function asRecord(value: unknown, context: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
}

function readPath(value: unknown, pathParts: string[], context: string): unknown {
  let cursor = value;
  for (const part of pathParts) {
    const record = asRecord(cursor, context);
    cursor = record[part];
  }
  return cursor;
}

function requireString(value: unknown, context: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${context} was not a string: ${JSON.stringify(value)}`);
  }
  return value;
}

function requireNoUserErrors(payload: unknown, pathParts: string[], context: string): void {
  const userErrors = readPath(payload, pathParts, context);
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors)}`);
}

async function capture(query: string, variables: JsonRecord): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  return {
    query,
    variables,
    status: result.status,
    response: result.payload,
  };
}

const cleanup: Array<Capture | { error: string }> = [];
let deleteCompleted = false;
let fixture:
  | (JsonRecord & {
      cleanup: Array<Capture | { error: string }>;
    })
  | null = null;

try {
  const shop = await capture(shopDocument, {});
  const shopId = requireString(readPath(shop.response, ['data', 'shop', 'id'], 'shop id'), 'shop id');
  const namespace = 'sdp_shop_owner';
  const key = `rw_${runId}`;
  const setVariables = {
    metafields: [
      {
        ownerId: shopId,
        namespace,
        key,
        type: 'single_line_text_field',
        value: `value-${runId}`,
      },
    ],
  };
  const readVariables = { namespace, key };
  const deleteVariables = {
    metafields: [
      {
        ownerId: shopId,
        namespace,
        key,
      },
    ],
  };
  const hydrate = await capture(ownerMetafieldsHydrateQuery, { ids: [shopId] });
  const set = await capture(setDocument, setVariables);
  requireNoUserErrors(set.response, ['data', 'metafieldsSet', 'userErrors'], 'metafieldsSet');
  const readAfterSet = await capture(readDocument, readVariables);
  const deleteResult = await capture(deleteDocument, deleteVariables);
  requireNoUserErrors(deleteResult.response, ['data', 'metafieldsDelete', 'userErrors'], 'metafieldsDelete');
  deleteCompleted = true;
  const readAfterDelete = await capture(readDocument, readVariables);

  fixture = {
    scenarioId: 'shop-owner-metafields-read-after-write',
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    shop,
    set,
    readAfterSet,
    delete: deleteResult,
    readAfterDelete,
    cleanup,
    upstreamCalls: [
      {
        operationName: 'OwnerMetafieldsHydrateNodes',
        variables: { ids: [shopId] },
        query: ownerMetafieldsHydrateQuery,
        response: { status: hydrate.status, body: hydrate.response },
      },
      {
        operationName: 'OwnerMetafieldsHydrateNodes',
        variables: { ids: [shopId] },
        query: ownerMetafieldsHydrateQuery,
        response: { status: hydrate.status, body: hydrate.response },
      },
    ],
  };
} finally {
  if (!deleteCompleted && fixture === null) {
    try {
      const shop = await capture(shopDocument, {});
      const shopId = requireString(
        readPath(shop.response, ['data', 'shop', 'id'], 'cleanup shop id'),
        'cleanup shop id',
      );
      cleanup.push(
        await capture(deleteDocument, {
          metafields: [{ ownerId: shopId, namespace: 'sdp_shop_owner', key: `rw_${runId}` }],
        }),
      );
    } catch (error) {
      cleanup.push({ error: error instanceof Error ? error.message : String(error) });
    }
  }
}

if (!fixture) {
  throw new Error('Shop owner metafields capture did not complete.');
}

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({ ok: true, outputPath, runId }, null, 2));
