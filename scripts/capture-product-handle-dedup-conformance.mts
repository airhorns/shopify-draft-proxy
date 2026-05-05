/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

type CapturedGraphqlResult = {
  status: number;
  payload: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-handle-dedup-parity.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateMutation = `#graphql
  mutation ProductHandleDedupCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDuplicateMutation = `#graphql
  mutation ProductHandleDedupDuplicate($productId: ID!, $newTitle: String!) {
    productDuplicate(productId: $productId, newTitle: $newTitle) {
      newProduct {
        id
        title
        handle
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const collectionCreateMutation = `#graphql
  mutation ProductHandleDedupCollectionCreate($input: CollectionInput!) {
    collectionCreate(input: $input) {
      collection {
        id
        title
        handle
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation ProductHandleDedupProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const collectionDeleteMutation = `#graphql
  mutation ProductHandleDedupCollectionDelete($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors {
        field
        message
      }
    }
  }
`;

async function capture(query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = (await runGraphqlRaw(query, variables)) as CapturedGraphqlResult;
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  return pathSegments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) {
      return null;
    }
    return (current as Record<string, unknown>)[segment] ?? null;
  }, value);
}

function assertGraphqlOk(capture: Capture, label: string): void {
  if (readPath(capture.response, ['errors'])) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(capture.response, null, 2)}`);
  }
}

function readProductId(capture: Capture, root: 'productCreate' | 'productDuplicate'): string | null {
  const productKey = root === 'productCreate' ? 'product' : 'newProduct';
  const id = readPath(capture.response, ['data', root, productKey, 'id']);
  return typeof id === 'string' && id.length > 0 ? id : null;
}

function readCollectionId(capture: Capture): string | null {
  const id = readPath(capture.response, ['data', 'collectionCreate', 'collection', 'id']);
  return typeof id === 'string' && id.length > 0 ? id : null;
}

async function cleanupProduct(id: string): Promise<Capture> {
  return capture(productDeleteMutation, { input: { id } });
}

async function cleanupCollection(id: string): Promise<Capture> {
  return capture(collectionDeleteMutation, { input: { id } });
}

const runId = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const productTitle = `HAR 579 Red Shirt ${runId}`;
const duplicateTitle = `${productTitle} Copy`;
const collectionTitle = `HAR 579 Red Collection ${runId}`;

const operations: Record<string, Capture> = {};
const cleanup: Capture[] = [];
const productIds: string[] = [];
const collectionIds: string[] = [];

try {
  operations.productCreateFirst = await capture(productCreateMutation, {
    product: { title: productTitle, status: 'DRAFT' },
  });
  assertGraphqlOk(operations.productCreateFirst, 'productCreateFirst');
  const sourceProductId = readProductId(operations.productCreateFirst, 'productCreate');
  if (!sourceProductId) {
    throw new Error('productCreateFirst did not return a source product id.');
  }
  productIds.push(sourceProductId);

  operations.productCreateSecond = await capture(productCreateMutation, {
    product: { title: productTitle, status: 'DRAFT' },
  });
  assertGraphqlOk(operations.productCreateSecond, 'productCreateSecond');
  const secondProductId = readProductId(operations.productCreateSecond, 'productCreate');
  if (secondProductId) productIds.push(secondProductId);

  operations.productCreateThird = await capture(productCreateMutation, {
    product: { title: productTitle, status: 'DRAFT' },
  });
  assertGraphqlOk(operations.productCreateThird, 'productCreateThird');
  const thirdProductId = readProductId(operations.productCreateThird, 'productCreate');
  if (thirdProductId) productIds.push(thirdProductId);

  operations.productCreateFourth = await capture(productCreateMutation, {
    product: { title: productTitle, status: 'DRAFT' },
  });
  assertGraphqlOk(operations.productCreateFourth, 'productCreateFourth');
  const fourthProductId = readProductId(operations.productCreateFourth, 'productCreate');
  if (fourthProductId) productIds.push(fourthProductId);

  operations.productCreateCopyFirst = await capture(productCreateMutation, {
    product: { title: duplicateTitle, status: 'DRAFT' },
  });
  assertGraphqlOk(operations.productCreateCopyFirst, 'productCreateCopyFirst');
  const firstCopyProductId = readProductId(operations.productCreateCopyFirst, 'productCreate');
  if (firstCopyProductId) productIds.push(firstCopyProductId);

  operations.productCreateCopySecond = await capture(productCreateMutation, {
    product: { title: duplicateTitle, status: 'DRAFT' },
  });
  assertGraphqlOk(operations.productCreateCopySecond, 'productCreateCopySecond');
  const secondCopyProductId = readProductId(operations.productCreateCopySecond, 'productCreate');
  if (secondCopyProductId) productIds.push(secondCopyProductId);

  operations.productDuplicateCopy = await capture(productDuplicateMutation, {
    productId: sourceProductId,
    newTitle: duplicateTitle,
  });
  assertGraphqlOk(operations.productDuplicateCopy, 'productDuplicateCopy');
  const duplicateProductId = readProductId(operations.productDuplicateCopy, 'productDuplicate');
  if (duplicateProductId) productIds.push(duplicateProductId);

  operations.collectionCreateFirst = await capture(collectionCreateMutation, {
    input: { title: collectionTitle },
  });
  assertGraphqlOk(operations.collectionCreateFirst, 'collectionCreateFirst');
  const firstCollectionId = readCollectionId(operations.collectionCreateFirst);
  if (firstCollectionId) collectionIds.push(firstCollectionId);

  operations.collectionCreateSecond = await capture(collectionCreateMutation, {
    input: { title: collectionTitle },
  });
  assertGraphqlOk(operations.collectionCreateSecond, 'collectionCreateSecond');
  const secondCollectionId = readCollectionId(operations.collectionCreateSecond);
  if (secondCollectionId) collectionIds.push(secondCollectionId);

  operations.collectionCreateThird = await capture(collectionCreateMutation, {
    input: { title: collectionTitle },
  });
  assertGraphqlOk(operations.collectionCreateThird, 'collectionCreateThird');
  const thirdCollectionId = readCollectionId(operations.collectionCreateThird);
  if (thirdCollectionId) collectionIds.push(thirdCollectionId);

  operations.collectionCreateFourth = await capture(collectionCreateMutation, {
    input: { title: collectionTitle },
  });
  assertGraphqlOk(operations.collectionCreateFourth, 'collectionCreateFourth');
  const fourthCollectionId = readCollectionId(operations.collectionCreateFourth);
  if (fourthCollectionId) collectionIds.push(fourthCollectionId);
} finally {
  for (const id of collectionIds.reverse()) {
    cleanup.push(await cleanupCollection(id));
  }
  for (const id of productIds.reverse()) {
    cleanup.push(await cleanupProduct(id));
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'product-handle-dedup',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      operations,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
