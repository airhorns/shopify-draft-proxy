/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: ConformanceGraphqlPayload;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-reference-fields-lifecycle.json');
const runId = Date.now().toString();

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const documents = {
  productCreate: await readFile(
    'config/parity-requests/metafields/metafield-reference-fields-product-create.graphql',
    'utf8',
  ),
  collectionCreate: await readFile(
    'config/parity-requests/metafields/metafield-reference-fields-collection-create.graphql',
    'utf8',
  ),
  fileCreate: await readFile(
    'config/parity-requests/metafields/metafield-reference-fields-file-create.graphql',
    'utf8',
  ),
  pageCreate: await readFile(
    'config/parity-requests/metafields/metafield-reference-fields-page-create.graphql',
    'utf8',
  ),
  metafieldsSet: await readFile('config/parity-requests/metafields/metafield-reference-fields-set.graphql', 'utf8'),
  read: await readFile('config/parity-requests/metafields/metafield-reference-fields-read.graphql', 'utf8'),
};

const cleanupDocuments = {
  productDelete: `#graphql
    mutation MetafieldReferenceFieldsCleanupProduct($input: ProductDeleteInput!) {
      productDelete(input: $input) {
        deletedProductId
        userErrors {
          field
          message
        }
      }
    }
  `,
  collectionDelete: `#graphql
    mutation MetafieldReferenceFieldsCleanupCollection($input: CollectionDeleteInput!) {
      collectionDelete(input: $input) {
        deletedCollectionId
        userErrors {
          field
          message
        }
      }
    }
  `,
  fileDelete: `#graphql
    mutation MetafieldReferenceFieldsCleanupFile($fileIds: [ID!]!) {
      fileDelete(fileIds: $fileIds) {
        deletedFileIds
        userErrors {
          field
          message
          code
        }
      }
    }
  `,
  pageDelete: `#graphql
    mutation MetafieldReferenceFieldsCleanupPage($id: ID!) {
      pageDelete(id: $id) {
        deletedPageId
        userErrors {
          field
          message
          code
        }
      }
    }
  `,
};

function readPath(value: unknown, pathSegments: Array<string | number>, context: string): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (typeof segment === 'number') {
      if (!Array.isArray(current)) {
        throw new Error(`${context}: expected array before segment ${segment}`);
      }
      current = current[segment];
    } else {
      if (typeof current !== 'object' || current === null || !(segment in current)) {
        throw new Error(`${context}: missing path segment ${segment}`);
      }
      current = (current as Record<string, unknown>)[segment];
    }
  }
  return current;
}

function readStringPath(value: unknown, pathSegments: Array<string | number>, context: string): string {
  const found = readPath(value, pathSegments, context);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`${context}: expected non-empty string at ${pathSegments.join('.')}`);
  }
  return found;
}

function assertNoTopLevelErrors(capture: Capture): void {
  if (capture.status < 200 || capture.status >= 300 || capture.response.errors) {
    throw new Error(`${capture.name} failed: ${JSON.stringify(capture.response, null, 2)}`);
  }
}

function assertNoUserErrors(capture: Capture, pathSegments: Array<string | number>): void {
  const userErrors = readPath(capture.response, pathSegments, capture.name);
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${capture.name} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

async function captureGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  const capture = {
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
  assertNoTopLevelErrors(capture);
  return capture;
}

async function cleanupGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  return {
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function buildProductVariables(label: string): Record<string, unknown> {
  return {
    product: {
      title: `Metafield reference ${label} ${runId}`,
      status: 'DRAFT',
    },
  };
}

const cleanup: Capture[] = [];
const created = {
  ownerProductId: null as string | null,
  targetProductId: null as string | null,
  collectionId: null as string | null,
  fileId: null as string | null,
  pageId: null as string | null,
};

try {
  const targetProduct = await captureGraphql(
    'target-product-create',
    documents.productCreate,
    buildProductVariables('target'),
  );
  assertNoUserErrors(targetProduct, ['data', 'productCreate', 'userErrors']);
  created.targetProductId = readStringPath(
    targetProduct.response,
    ['data', 'productCreate', 'product', 'id'],
    'target product id',
  );
  const variantId = readStringPath(
    targetProduct.response,
    ['data', 'productCreate', 'product', 'variants', 'nodes', 0, 'id'],
    'target product default variant id',
  );

  const ownerProduct = await captureGraphql(
    'owner-product-create',
    documents.productCreate,
    buildProductVariables('owner'),
  );
  assertNoUserErrors(ownerProduct, ['data', 'productCreate', 'userErrors']);
  created.ownerProductId = readStringPath(
    ownerProduct.response,
    ['data', 'productCreate', 'product', 'id'],
    'owner product id',
  );

  const collection = await captureGraphql('target-collection-create', documents.collectionCreate, {
    input: {
      title: `Metafield reference collection ${runId}`,
    },
  });
  assertNoUserErrors(collection, ['data', 'collectionCreate', 'userErrors']);
  created.collectionId = readStringPath(
    collection.response,
    ['data', 'collectionCreate', 'collection', 'id'],
    'collection id',
  );

  const file = await captureGraphql('target-file-create', documents.fileCreate, {
    files: [
      {
        alt: `Metafield reference image ${runId}`,
        contentType: 'IMAGE',
        filename: `metafield-reference-${runId}.png`,
        originalSource: `https://placehold.co/600x400.png?text=metafield-reference-${runId}`,
      },
    ],
  });
  assertNoUserErrors(file, ['data', 'fileCreate', 'userErrors']);
  created.fileId = readStringPath(file.response, ['data', 'fileCreate', 'files', 0, 'id'], 'file id');

  const page = await captureGraphql('target-page-create', documents.pageCreate, {
    page: {
      title: `Metafield reference page ${runId}`,
      body: '<p>Metafield reference page body.</p>',
    },
  });
  assertNoUserErrors(page, ['data', 'pageCreate', 'userErrors']);
  created.pageId = readStringPath(page.response, ['data', 'pageCreate', 'page', 'id'], 'page id');

  const setReferences = await captureGraphql('reference-metafields-set', documents.metafieldsSet, {
    metafields: [
      {
        ownerId: created.ownerProductId,
        namespace: 'reference_fields',
        key: 'product_ref',
        type: 'product_reference',
        value: created.targetProductId,
      },
      {
        ownerId: created.ownerProductId,
        namespace: 'reference_fields',
        key: 'product_refs',
        type: 'list.product_reference',
        value: JSON.stringify([created.targetProductId]),
      },
      {
        ownerId: created.ownerProductId,
        namespace: 'reference_fields',
        key: 'variant_ref',
        type: 'variant_reference',
        value: variantId,
      },
      {
        ownerId: created.ownerProductId,
        namespace: 'reference_fields',
        key: 'collection_ref',
        type: 'collection_reference',
        value: created.collectionId,
      },
      {
        ownerId: created.ownerProductId,
        namespace: 'reference_fields',
        key: 'file_ref',
        type: 'file_reference',
        value: created.fileId,
      },
      {
        ownerId: created.ownerProductId,
        namespace: 'reference_fields',
        key: 'page_ref',
        type: 'page_reference',
        value: created.pageId,
      },
      {
        ownerId: created.ownerProductId,
        namespace: 'reference_fields',
        key: 'plain',
        type: 'single_line_text_field',
        value: 'not a reference',
      },
    ],
  });
  assertNoUserErrors(setReferences, ['data', 'metafieldsSet', 'userErrors']);

  const referenceRead = await captureGraphql('reference-fields-read', documents.read, {
    ownerId: created.ownerProductId,
  });

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        summary:
          'Live Shopify capture for product-owned Metafield.reference and Metafield.references resolving product, variant, collection, file, and page reference targets.',
        setup: {
          targetProduct,
          ownerProduct,
          collection,
          file,
          page,
        },
        mutation: {
          setReferences,
        },
        read: referenceRead,
        cleanup,
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
  console.log(`Wrote ${outputPath}`);
} finally {
  if (created.ownerProductId) {
    cleanup.push(
      await cleanupGraphql('cleanup-owner-product-delete', cleanupDocuments.productDelete, {
        input: { id: created.ownerProductId },
      }),
    );
  }
  if (created.targetProductId) {
    cleanup.push(
      await cleanupGraphql('cleanup-target-product-delete', cleanupDocuments.productDelete, {
        input: { id: created.targetProductId },
      }),
    );
  }
  if (created.collectionId) {
    cleanup.push(
      await cleanupGraphql('cleanup-collection-delete', cleanupDocuments.collectionDelete, {
        input: { id: created.collectionId },
      }),
    );
  }
  if (created.fileId) {
    cleanup.push(
      await cleanupGraphql('cleanup-file-delete', cleanupDocuments.fileDelete, {
        fileIds: [created.fileId],
      }),
    );
  }
  if (created.pageId) {
    cleanup.push(
      await cleanupGraphql('cleanup-page-delete', cleanupDocuments.pageDelete, {
        id: created.pageId,
      }),
    );
  }
}
