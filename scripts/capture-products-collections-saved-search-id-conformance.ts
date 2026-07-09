/* oxlint-disable no-console -- CLI capture scripts report progress and output paths. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type CaptureOperation = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  status: number;
  response: ConformanceGraphqlPayload<JsonRecord>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'products-collections-saved-search-id.json');
const requestDir = path.join('config', 'parity-requests', 'products');

const productCreateDocumentPath = path.join(requestDir, 'saved-search-id-product-create.graphql');
const collectionCreateDocumentPath = path.join(requestDir, 'saved-search-id-collection-create.graphql');
const savedSearchCreateDocumentPath = path.join(requestDir, 'saved-search-id-saved-search-create.graphql');
const readDocumentPath = path.join(requestDir, 'saved-search-id-read.graphql');
const connectionConflictDocumentPath = path.join(requestDir, 'saved-search-id-connection-conflict.graphql');
const countConflictDocumentPath = path.join(requestDir, 'saved-search-id-count-conflict.graphql');
const connectionUnknownDocumentPath = path.join(requestDir, 'saved-search-id-connection-unknown.graphql');
const countUnknownDocumentPath = path.join(requestDir, 'saved-search-id-count-unknown.graphql');

const productDeleteDocument = `mutation SavedSearchIdProductCleanup($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}
`;

const collectionDeleteDocument = `mutation SavedSearchIdCollectionCleanup($input: CollectionDeleteInput!) {
  collectionDelete(input: $input) {
    deletedCollectionId
    userErrors {
      field
      message
    }
  }
}
`;

const savedSearchDeleteDocument = `mutation SavedSearchIdSavedSearchCleanup($input: SavedSearchDeleteInput!) {
  savedSearchDelete(input: $input) {
    deletedSavedSearchId
    userErrors {
      field
      message
    }
  }
}
`;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let cursor = value;
  for (const pathPart of pathParts) {
    if (!isRecord(cursor)) return undefined;
    cursor = cursor[pathPart];
  }
  return cursor;
}

function requireStringPath(value: unknown, pathParts: string[], label: string): string {
  const found = readPath(value, pathParts);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`Expected ${label} at ${pathParts.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return found;
}

function gidTail(id: string): string {
  return id.split('?')[0]?.split('/').pop() ?? id;
}

async function readDocument(documentPath: string): Promise<string> {
  return await readFile(documentPath, 'utf8');
}

async function captureGraphql(query: string, variables: JsonRecord): Promise<CaptureOperation> {
  const result = await runGraphqlRequest<JsonRecord>(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`GraphQL HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

const createdProductIds: string[] = [];
const createdCollectionIds: string[] = [];
const createdSavedSearchIds: string[] = [];
const cleanup: CaptureOperation[] = [];

try {
  const stamp = Date.now().toString().slice(-8);
  const runId = `saved-search-id-${stamp}`;
  const productCreateDocument = await readDocument(productCreateDocumentPath);
  const collectionCreateDocument = await readDocument(collectionCreateDocumentPath);
  const savedSearchCreateDocument = await readDocument(savedSearchCreateDocumentPath);
  const readDocumentText = await readDocument(readDocumentPath);
  const connectionConflictDocument = await readDocument(connectionConflictDocumentPath);
  const countConflictDocument = await readDocument(countConflictDocumentPath);
  const connectionUnknownDocument = await readDocument(connectionUnknownDocumentPath);
  const countUnknownDocument = await readDocument(countUnknownDocumentPath);

  const productCreate = await captureGraphql(productCreateDocument, {
    product: {
      title: `${runId} product`,
      vendor: runId,
      productType: 'SavedSearchId',
      tags: [`ssid-${stamp}`],
    },
  });
  const productId = requireStringPath(
    productCreate.response,
    ['data', 'productCreate', 'product', 'id'],
    'created product id',
  );
  createdProductIds.push(productId);

  const collectionCreate = await captureGraphql(collectionCreateDocument, {
    input: {
      title: `${runId} collection`,
    },
  });
  const collectionId = requireStringPath(
    collectionCreate.response,
    ['data', 'collectionCreate', 'collection', 'id'],
    'created collection id',
  );
  createdCollectionIds.push(collectionId);

  const productQuery = `id:${gidTail(productId)}`;
  const collectionQuery = `id:${gidTail(collectionId)}`;
  const productSavedSearchCreate = await captureGraphql(savedSearchCreateDocument, {
    input: {
      name: `SSIP${stamp}`,
      query: productQuery,
      resourceType: 'PRODUCT',
    },
  });
  const productSavedSearchId = requireStringPath(
    productSavedSearchCreate.response,
    ['data', 'savedSearchCreate', 'savedSearch', 'id'],
    'product saved search id',
  );
  createdSavedSearchIds.push(productSavedSearchId);

  const collectionSavedSearchCreate = await captureGraphql(savedSearchCreateDocument, {
    input: {
      name: `SSIC${stamp}`,
      query: collectionQuery,
      resourceType: 'COLLECTION',
    },
  });
  const collectionSavedSearchId = requireStringPath(
    collectionSavedSearchCreate.response,
    ['data', 'savedSearchCreate', 'savedSearch', 'id'],
    'collection saved search id',
  );
  createdSavedSearchIds.push(collectionSavedSearchId);

  const validRead = await captureGraphql(readDocumentText, {
    productSavedSearchId,
    productQuery,
    collectionSavedSearchId,
    collectionQuery,
  });
  const connectionConflict = await captureGraphql(connectionConflictDocument, {
    productSavedSearchId,
    collectionSavedSearchId,
    query: 'title:beta',
  });
  const countConflict = await captureGraphql(countConflictDocument, {
    productSavedSearchId,
    collectionSavedSearchId,
    query: 'title:beta',
  });
  const unknownId = 'gid://shopify/SavedSearch/0';
  const connectionUnknown = await captureGraphql(connectionUnknownDocument, { unknownId });
  const countUnknown = await captureGraphql(countUnknownDocument, { unknownId });

  for (const id of createdSavedSearchIds) {
    cleanup.push(await captureGraphql(savedSearchDeleteDocument, { input: { id } }));
  }
  createdSavedSearchIds.splice(0, createdSavedSearchIds.length);

  for (const id of createdCollectionIds) {
    cleanup.push(await captureGraphql(collectionDeleteDocument, { input: { id } }));
  }
  createdCollectionIds.splice(0, createdCollectionIds.length);

  for (const id of createdProductIds) {
    cleanup.push(await captureGraphql(productDeleteDocument, { input: { id } }));
  }
  createdProductIds.splice(0, createdProductIds.length);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId: 'products-collections-saved-search-id',
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        operations: {
          productCreate,
          collectionCreate,
          productSavedSearchCreate,
          collectionSavedSearchCreate,
          validRead,
          connectionConflict,
          countConflict,
          connectionUnknown,
          countUnknown,
        },
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
  console.log(`Wrote ${outputPath}`);
} finally {
  for (const id of createdSavedSearchIds) {
    try {
      await runGraphqlRequest(savedSearchDeleteDocument, { input: { id } });
    } catch (error) {
      console.error(`Failed to cleanup saved search ${id}`, error);
    }
  }
  for (const id of createdCollectionIds) {
    try {
      await runGraphqlRequest(collectionDeleteDocument, { input: { id } });
    } catch (error) {
      console.error(`Failed to cleanup collection ${id}`, error);
    }
  }
  for (const id of createdProductIds) {
    try {
      await runGraphqlRequest(productDeleteDocument, { input: { id } });
    } catch (error) {
      console.error(`Failed to cleanup product ${id}`, error);
    }
  }
}
