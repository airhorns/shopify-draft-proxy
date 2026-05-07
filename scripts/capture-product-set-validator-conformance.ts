/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productSetShapeMutation = `#graphql
  mutation ProductSetShapeValidatorParity($input: ProductSetInput!, $synchronous: Boolean!) {
    productSet(input: $input, synchronous: $synchronous) {
      product {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const productSetIdentifierMutation = `#graphql
  mutation ProductSetIdentifierIdNotAllowed($identifier: ProductSetIdentifiers, $input: ProductSetInput!, $synchronous: Boolean!) {
    productSet(identifier: $identifier, input: $input, synchronous: $synchronous) {
      product {
        id
        title
        handle
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const productSetAsyncMutation = `#graphql
  mutation ProductSetAsyncOperationParity($input: ProductSetInput!, $synchronous: Boolean!) {
    productSet(input: $input, synchronous: $synchronous) {
      product {
        id
      }
      productSetOperation {
        id
        status
        product {
          id
          title
          handle
        }
        userErrors {
          field
          message
          code
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const productOperationReadQuery = `#graphql
  query ProductSetOperationRead($id: ID!) {
    productOperation(id: $id) {
      __typename
      status
      product {
        id
        title
        handle
      }
      ... on ProductSetOperation {
        id
        userErrors {
          field
          message
          code
        }
      }
    }
  }
`;

const locationsQuery = `#graphql
  query ProductSetValidatorLocations {
    locations(first: 1) {
      nodes {
        id
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation ProductSetValidatorCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    if (Array.isArray(current)) {
      const index = Number.parseInt(part, 10);
      current = Number.isNaN(index) ? undefined : current[index];
    } else {
      current = readRecord(current)?.[part];
    }
  }
  return current;
}

function responseData(response: { data?: unknown }): JsonRecord {
  return readRecord(response.data) ?? {};
}

function buildVariantLimitVariables(runId: string): JsonRecord {
  return {
    synchronous: true,
    input: {
      title: `Hermes ProductSet Variant Limit ${runId}`,
      vendor: 'Hermes',
      variants: Array.from({ length: 2049 }, (_, index) => ({
        price: '1.00',
        optionValues: [{ optionName: 'Title', name: `Variant ${index + 1}` }],
      })),
    },
  };
}

function buildInventoryQuantitiesLimitVariables(runId: string, locationId: string): JsonRecord {
  return {
    synchronous: true,
    input: {
      title: `Hermes ProductSet Inventory Limit ${runId}`,
      vendor: 'Hermes',
      productOptions: [{ name: 'Title', position: 1, values: [{ name: 'Default Title' }] }],
      variants: [
        {
          price: '1.00',
          optionValues: [{ optionName: 'Title', name: 'Default Title' }],
          inventoryQuantities: Array.from({ length: 251 }, () => ({
            locationId,
            name: 'available',
            quantity: 1,
          })),
        },
      ],
    },
  };
}

function buildUnknownProductVariables(): JsonRecord {
  return {
    synchronous: true,
    input: {
      id: 'gid://shopify/Product/999999999999',
      title: 'Hermes ProductSet Missing Product',
      vendor: 'Hermes',
    },
  };
}

function buildIdentifierIdNotAllowedVariables(runId: string): JsonRecord {
  const handle = `hermes-product-set-id-not-allowed-${runId}`;
  return {
    synchronous: true,
    identifier: {
      handle,
    },
    input: {
      id: 'gid://shopify/Product/999999999999',
      title: `Hermes ProductSet ID Not Allowed ${runId}`,
      handle,
      vendor: 'Hermes',
    },
  };
}

function buildMissingIdentifierVariables(runId: string): JsonRecord {
  return {
    synchronous: true,
    identifier: null,
    input: {
      title: `Hermes ProductSet Missing Identifier ${runId}`,
      vendor: 'Hermes',
    },
  };
}

function buildAsyncVariables(runId: string): JsonRecord {
  return {
    synchronous: false,
    input: {
      title: `Hermes ProductSet Async ${runId}`,
      vendor: 'Hermes',
      status: 'DRAFT',
    },
  };
}

async function cleanupProduct(productId: string | null): Promise<unknown> {
  if (!productId) {
    return null;
  }
  try {
    return await runGraphql(productDeleteMutation, { input: { id: productId } });
  } catch (error) {
    console.warn(`cleanup failed for ${productId}:`, error);
    return null;
  }
}

await mkdir(outputDir, { recursive: true });

const runId = Date.now().toString();
const locationsResponse = await runGraphql(locationsQuery, {});
const locationId = readPath(responseData(locationsResponse), ['locations', 'nodes', '0', 'id']);
if (typeof locationId !== 'string') {
  throw new Error('Could not resolve a writable location id for productSet inventoryQuantities validation capture.');
}

const variantLimitVariables = buildVariantLimitVariables(runId);
const variantLimitResponse = (await runGraphqlRaw(productSetShapeMutation, variantLimitVariables)).payload;
const inventoryQuantitiesLimitVariables = buildInventoryQuantitiesLimitVariables(runId, locationId);
const inventoryQuantitiesLimitResponse = (
  await runGraphqlRaw(productSetShapeMutation, inventoryQuantitiesLimitVariables)
).payload;
const unknownProductVariables = buildUnknownProductVariables();
const unknownProductResponse = (await runGraphqlRaw(productSetShapeMutation, unknownProductVariables)).payload;

const identifierIdNotAllowedVariables = buildIdentifierIdNotAllowedVariables(runId);
const identifierIdNotAllowedResponse = (
  await runGraphqlRaw(productSetIdentifierMutation, identifierIdNotAllowedVariables)
).payload;
const missingIdentifierVariables = buildMissingIdentifierVariables(runId);
const missingIdentifierResponse = (await runGraphqlRaw(productSetIdentifierMutation, missingIdentifierVariables))
  .payload;
const missingIdentifierProductId = readPath(responseData(missingIdentifierResponse), ['productSet', 'product', 'id']);

let asyncProductId: string | null = null;
try {
  const asyncVariables = buildAsyncVariables(runId);
  const asyncResponse = (await runGraphqlRaw(productSetAsyncMutation, asyncVariables)).payload;
  const operationId = readPath(responseData(asyncResponse), ['productSet', 'productSetOperation', 'id']);
  if (typeof operationId !== 'string') {
    throw new Error('productSet async capture did not return a ProductSetOperation id.');
  }
  let operationReadResponse = (await runGraphqlRaw(productOperationReadQuery, { id: operationId })).payload;
  for (
    let attempt = 0;
    attempt < 5 && readPath(responseData(operationReadResponse), ['productOperation', 'status']) !== 'COMPLETE';
    attempt += 1
  ) {
    await new Promise((resolve) => setTimeout(resolve, 1000));
    operationReadResponse = (await runGraphqlRaw(productOperationReadQuery, { id: operationId })).payload;
  }
  const productId = readPath(responseData(operationReadResponse), ['productOperation', 'product', 'id']);
  asyncProductId = typeof productId === 'string' ? productId : null;

  await writeFile(
    path.join(outputDir, 'product-set-async-operation-parity.json'),
    `${JSON.stringify(
      {
        mutation: {
          variables: asyncVariables,
          response: asyncResponse,
        },
        operationRead: {
          variables: { id: operationId },
          response: operationReadResponse,
        },
        cleanup: null,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
} finally {
  const cleanup = await cleanupProduct(asyncProductId);
  if (asyncProductId) {
    // Keep cleanup evidence in stdout rather than mutating the already-written comparison fixture.
    console.log(JSON.stringify({ cleanupProductId: asyncProductId, cleanup }, null, 2));
  }
}

if (typeof missingIdentifierProductId === 'string') {
  const cleanup = await cleanupProduct(missingIdentifierProductId);
  console.log(JSON.stringify({ cleanupProductId: missingIdentifierProductId, cleanup }, null, 2));
}

await writeFile(
  path.join(outputDir, 'product-set-shape-validator-parity.json'),
  `${JSON.stringify(
    {
      variantLimit: {
        variables: variantLimitVariables,
        response: variantLimitResponse,
      },
      inventoryQuantitiesLimit: {
        variables: inventoryQuantitiesLimitVariables,
        response: inventoryQuantitiesLimitResponse,
      },
      unknownProduct: {
        variables: unknownProductVariables,
        response: unknownProductResponse,
      },
      locations: {
        response: locationsResponse,
      },
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);

await writeFile(
  path.join(outputDir, 'product-set-id-not-allowed.json'),
  `${JSON.stringify(
    {
      idNotAllowed: {
        variables: identifierIdNotAllowedVariables,
        response: identifierIdNotAllowedResponse,
      },
      missingIdentifier: {
        variables: missingIdentifierVariables,
        response: missingIdentifierResponse,
      },
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);

console.log(`Wrote productSet validator captures under ${outputDir}`);
