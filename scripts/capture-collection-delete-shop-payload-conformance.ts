/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productsDir = path.join('config', 'parity-requests', 'products');
const specsDir = path.join('config', 'parity-specs', 'products');
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');

const documentPath = path.join(productsDir, 'collectionDelete-parity-plan.graphql');
const specPath = path.join(specsDir, 'collectionDelete-parity-plan.json');
const fixturePath = path.join(fixtureDir, 'collection-delete-shop-payload.json');

const collectionDeleteDocument = `mutation CollectionDeleteParityPlan($input: CollectionDeleteInput!) {
  collectionDelete(input: $input) {
    deletedCollectionId
    shop {
      id
    }
    userErrors {
      field
      message
    }
  }
}
`;

const collectionCreateDocument = `mutation CollectionDeleteShopPayloadSetup($input: CollectionInput!) {
  collectionCreate(input: $input) {
    collection {
      id
      title
      handle
      products(first: 10) {
        nodes {
          id
          title
          handle
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
        }
      }
      defaultProducts: products(first: 10, sortKey: COLLECTION_DEFAULT) {
        nodes {
          id
          title
          handle
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
        }
      }
      manualProducts: products(first: 10, sortKey: MANUAL) {
        nodes {
          id
          title
          handle
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
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

const productsHydrateNodesDocument =
  'query ProductsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { ... on Product { id title handle status totalInventory tracksInventory variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } collections(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } ... on Collection { id title handle products(first: 10) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } defaultProducts: products(first: 10, sortKey: COLLECTION_DEFAULT) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } manualProducts: products(first: 10, sortKey: MANUAL) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } } } ... on ProductVariant { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } product { id title handle status totalInventory tracksInventory variants(first: 10) { nodes { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } } } } } } }';

const downstreamReadDocument = `query CollectionDeleteDownstreamRead($id: ID!) {
  collection(id: $id) {
    id
  }
}
`;

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, segments: readonly string[]): unknown {
  let current: unknown = value;
  for (const segment of segments) {
    const record = readRecord(current);
    if (!record) return undefined;
    current = record[segment];
  }
  return current;
}

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} failed with HTTP ${result.status}: ${JSON.stringify(result.payload)}`);
  }
  if (result.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result.payload.errors)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathSegments: readonly string[], label: string): void {
  const userErrors = readPath(payload, pathSegments);
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function assertShopPayload(payload: unknown, pathSegments: readonly string[], label: string): void {
  const shopId = readPath(payload, [...pathSegments, 'shop', 'id']);
  if (typeof shopId !== 'string' || !shopId.startsWith('gid://shopify/Shop/')) {
    throw new Error(`${label} did not return shop { id }: ${JSON.stringify(readPath(payload, pathSegments))}`);
  }
}

const runId = Date.now().toString();
let createdCollectionId: string | null = null;
const missingCollectionId = `gid://shopify/Collection/${runId}999`;

try {
  const createVariables = {
    input: {
      title: `Collection delete shop payload ${runId}`,
      sortOrder: 'MANUAL',
    },
  };
  const create = await runGraphqlRequest<JsonRecord>(collectionCreateDocument, createVariables);
  assertHttpOk(create, 'collectionDelete shop payload setup');
  assertNoUserErrors(create.payload, ['data', 'collectionCreate', 'userErrors'], 'collectionCreate setup');

  const createdId = readPath(create.payload, ['data', 'collectionCreate', 'collection', 'id']);
  if (typeof createdId !== 'string') {
    throw new Error(`collectionCreate setup did not return a collection id: ${JSON.stringify(create.payload)}`);
  }
  createdCollectionId = createdId;

  const successVariables = { input: { id: createdCollectionId } };
  const successHydrate = await runGraphqlRequest<JsonRecord>(productsHydrateNodesDocument, {
    ids: [createdCollectionId],
  });
  assertHttpOk(successHydrate, 'collectionDelete success hydrate');

  const success = await runGraphqlRequest<JsonRecord>(collectionDeleteDocument, successVariables);
  assertHttpOk(success, 'collectionDelete success');
  assertNoUserErrors(success.payload, ['data', 'collectionDelete', 'userErrors'], 'collectionDelete success');
  assertShopPayload(success.payload, ['data', 'collectionDelete'], 'collectionDelete success');
  createdCollectionId = null;

  const downstreamReadVariables = { id: successVariables.input.id };
  const downstreamRead = await runGraphqlRequest<JsonRecord>(downstreamReadDocument, downstreamReadVariables);
  assertHttpOk(downstreamRead, 'collectionDelete downstream read');

  const notFoundVariables = { input: { id: missingCollectionId } };
  const notFoundHydrate = await runGraphqlRequest<JsonRecord>(productsHydrateNodesDocument, {
    ids: [missingCollectionId],
  });
  assertHttpOk(notFoundHydrate, 'collectionDelete not-found hydrate');

  const notFound = await runGraphqlRequest<JsonRecord>(collectionDeleteDocument, notFoundVariables);
  assertHttpOk(notFound, 'collectionDelete not-found');
  assertShopPayload(notFound.payload, ['data', 'collectionDelete'], 'collectionDelete not-found');
  if (readPath(notFound.payload, ['data', 'collectionDelete', 'deletedCollectionId']) !== null) {
    throw new Error(`collectionDelete not-found returned a deleted id: ${JSON.stringify(notFound.payload)}`);
  }
  const notFoundUserErrors = readPath(notFound.payload, ['data', 'collectionDelete', 'userErrors']);
  if (!Array.isArray(notFoundUserErrors) || notFoundUserErrors.length === 0) {
    throw new Error(`collectionDelete not-found did not return userErrors: ${JSON.stringify(notFound.payload)}`);
  }

  await mkdir(productsDir, { recursive: true });
  await mkdir(specsDir, { recursive: true });
  await mkdir(fixtureDir, { recursive: true });

  await writeFile(documentPath, collectionDeleteDocument, 'utf8');
  await writeFile(
    fixturePath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        apiVersion,
        storeDomain,
        success: {
          document: collectionDeleteDocument,
          variables: successVariables,
          response: success.payload,
        },
        notFound: {
          document: collectionDeleteDocument,
          variables: notFoundVariables,
          response: notFound.payload,
        },
        downstreamRead: {
          document: downstreamReadDocument,
          variables: downstreamReadVariables,
          response: downstreamRead.payload,
        },
        upstreamCalls: [
          {
            operationName: 'ProductsHydrateNodes',
            variables: { ids: [successVariables.input.id] },
            query: productsHydrateNodesDocument,
            response: {
              status: successHydrate.status,
              body: successHydrate.payload,
            },
          },
          {
            operationName: 'ProductsHydrateNodes',
            variables: { ids: [missingCollectionId] },
            query: productsHydrateNodesDocument,
            response: {
              status: notFoundHydrate.status,
              body: notFoundHydrate.payload,
            },
          },
        ],
        notes: [
          'Live public Admin GraphQL 2026-04 returns a non-null CollectionDeletePayload.shop on both success and not-found userError branches.',
          'The upstreamCalls entries are real live ProductsHydrateNodes reads captured before replay so parity can hydrate collection existence without writing to Shopify.',
        ],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
  await writeFile(
    specPath,
    `${JSON.stringify(
      {
        scenarioId: 'collection-delete-live-parity',
        operationNames: ['collectionDelete'],
        scenarioStatus: 'captured',
        assertionKinds: ['payload-shape', 'user-errors-parity', 'downstream-read-parity'],
        liveCaptureFiles: [fixturePath],
        proxyRequest: {
          documentPath,
          variablesPath: path.join(productsDir, 'collectionDelete-parity-plan.variables.json'),
          variablesCapturePath: '$.success.variables',
        },
        comparisonMode: 'captured-vs-proxy-request',
        notes:
          'Executed by the conformance parity runner against the local staged-mutation path with live 2026-04 fixture variables. The payload selects shop { id } on both the successful delete and not-found userError branches; shop.id is treated as store-specific identity while preserving the required non-null shop object.',
        comparison: {
          mode: 'strict-json',
          expectedDifferences: [],
          targets: [
            {
              name: 'success-payload-includes-shop',
              capturePath: '$.success.response.data',
              proxyPath: '$.data',
              expectedDifferences: [
                {
                  path: '$.collectionDelete.shop.id',
                  matcher: 'shopify-gid:Shop',
                  reason:
                    "The live capture uses the dev store's shop gid; an otherwise empty local proxy returns its stable synthetic shop gid while preserving the non-null Shop payload.",
                },
              ],
            },
            {
              name: 'not-found-payload-includes-shop',
              capturePath: '$.notFound.response.data',
              proxyPath: '$.data',
              proxyRequest: {
                documentPath,
                variablesCapturePath: '$.notFound.variables',
              },
              expectedDifferences: [
                {
                  path: '$.collectionDelete.shop.id',
                  matcher: 'shopify-gid:Shop',
                  reason:
                    "The live capture uses the dev store's shop gid; an otherwise empty local proxy returns its stable synthetic shop gid while preserving the non-null Shop payload.",
                },
              ],
            },
            {
              name: 'downstream-read-after-delete',
              capturePath: '$.downstreamRead.response.data.collection',
              proxyPath: '$.data.collection',
              proxyRequest: {
                documentCapturePath: '$.downstreamRead.document',
                variablesCapturePath: '$.downstreamRead.variables',
              },
            },
          ],
        },
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(`Wrote ${documentPath}`);
  console.log(`Wrote ${fixturePath}`);
  console.log(`Wrote ${specPath}`);
} finally {
  if (createdCollectionId) {
    try {
      await runGraphqlRequest(collectionDeleteDocument, { input: { id: createdCollectionId } });
    } catch {
      // Best-effort cleanup only; preserve the original capture failure.
    }
  }
}
