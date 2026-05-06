/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type CapturedGraphqlResponse = {
  data?: Record<string, any>;
};

function responseData(response: unknown): Record<string, any> {
  return (response as CapturedGraphqlResponse).data ?? {};
}

const productSetMutation = `#graphql
  mutation ProductDeleteAsyncSourceCreate($input: ProductSetInput!, $synchronous: Boolean!) {
    productSet(input: $input, synchronous: $synchronous) {
      product {
        id
        title
        handle
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteAsyncMutation = `#graphql
  mutation ProductDeleteAsyncOperation($input: ProductDeleteInput!, $synchronous: Boolean!) {
    productDelete(input: $input, synchronous: $synchronous) {
      deletedProductId
      productDeleteOperation {
        id
        status
        deletedProductId
        userErrors {
          field
          message
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteSyncMutation = `#graphql
  mutation ProductDeleteAsyncCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const productReadQuery = `#graphql
  query ProductDeleteAsyncProductRead($id: ID!) {
    product(id: $id) {
      id
      title
      handle
      status
    }
  }
`;

const operationReadQuery = `#graphql
  query ProductDeleteOperationRead($id: ID!) {
    productOperation(id: $id) {
      __typename
      ... on ProductDeleteOperation {
        id
        status
        deletedProductId
        userErrors {
          field
          message
        }
      }
    }
  }
`;

const nodeReadQuery = `#graphql
  query ProductDeleteOperationNodeRead($id: ID!) {
    node(id: $id) {
      __typename
      ... on ProductDeleteOperation {
        id
        status
        deletedProductId
        userErrors {
          field
          message
        }
      }
    }
  }
`;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function expectNoUserErrors(pathLabel: string, userErrors: unknown): void {
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }

  throw new Error(`${pathLabel} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function expectPendingJobError(pathLabel: string, userErrors: unknown): void {
  if (
    Array.isArray(userErrors) &&
    userErrors.some((error) => {
      const record = error as { field?: unknown; message?: unknown };
      return (
        (record.field === null || (Array.isArray(record.field) && record.field.join('.') === 'base')) &&
        typeof record.message === 'string' &&
        record.message.toLowerCase().includes('operation')
      );
    })
  ) {
    return;
  }

  throw new Error(
    `${pathLabel} did not return a base product-operation userError: ${JSON.stringify(userErrors ?? null)}`,
  );
}

async function readProduct(productId: string): Promise<CapturedGraphqlResponse> {
  return (await runGraphql(productReadQuery, { id: productId })) as CapturedGraphqlResponse;
}

async function readOperation(operationId: string): Promise<CapturedGraphqlResponse> {
  return (await runGraphql(operationReadQuery, { id: operationId })) as CapturedGraphqlResponse;
}

async function deleteProduct(productId: string | null): Promise<unknown | null> {
  if (!productId) {
    return null;
  }

  try {
    return await runGraphql(productDeleteSyncMutation, { input: { id: productId } });
  } catch (error) {
    return {
      cleanupError: error instanceof Error ? error.message : String(error),
    };
  }
}

async function waitForAsyncDeleteCleanup(productId: string | null, operationId: string | null): Promise<unknown> {
  if (!productId) {
    return null;
  }

  const polls: unknown[] = [];
  for (let attempt = 0; attempt < 12; attempt += 1) {
    const productRead = await readProduct(productId);
    const operationRead = operationId ? await readOperation(operationId) : null;
    polls.push({ productRead, operationRead });

    const product = responseData(productRead)['product'];
    if (product === null) {
      return { polls, finalProductDeleted: true };
    }

    await sleep(2000);
  }

  return {
    polls,
    finalProductDeleted: false,
    fallbackDelete: await deleteProduct(productId),
  };
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
let sourceProductId: string | null = null;
let operationId: string | null = null;
let cleanup: unknown | null = null;

try {
  const sourceCreateVariables = {
    synchronous: true,
    input: {
      title: `Async Delete Source ${runId}`,
      status: 'DRAFT',
    },
  };
  const sourceCreateResponse = (await runGraphql(productSetMutation, sourceCreateVariables)) as CapturedGraphqlResponse;
  expectNoUserErrors(
    'productSet async delete source create',
    responseData(sourceCreateResponse)['productSet']?.userErrors,
  );
  sourceProductId = responseData(sourceCreateResponse)['productSet']?.product?.id ?? null;
  if (!sourceProductId) {
    throw new Error('Async productDelete capture could not create a disposable source product.');
  }

  const sourceReadBeforeDelete = await readProduct(sourceProductId);
  const deleteVariables = {
    input: { id: sourceProductId },
    synchronous: false,
  };
  const mutationResponse = (await runGraphql(productDeleteAsyncMutation, deleteVariables)) as CapturedGraphqlResponse;
  expectNoUserErrors('async productDelete mutation', responseData(mutationResponse)['productDelete']?.userErrors);
  operationId = responseData(mutationResponse)['productDelete']?.productDeleteOperation?.id ?? null;
  if (!operationId) {
    throw new Error('Async productDelete capture did not return a ProductDeleteOperation id.');
  }

  const duplicateMutationResponse = (await runGraphql(
    productDeleteAsyncMutation,
    deleteVariables,
  )) as CapturedGraphqlResponse;
  expectPendingJobError(
    'duplicate async productDelete mutation',
    responseData(duplicateMutationResponse)['productDelete']?.userErrors,
  );

  const downstreamRead = await readProduct(sourceProductId);
  const operationRead = await readOperation(operationId);
  const nodeRead = (await runGraphql(nodeReadQuery, { id: operationId })) as CapturedGraphqlResponse;

  cleanup = await waitForAsyncDeleteCleanup(sourceProductId, operationId);

  const capture = {
    storeDomain,
    apiVersion,
    setup: {
      sourceCreate: {
        variables: sourceCreateVariables,
        response: sourceCreateResponse,
      },
      sourceProductId,
      sourceReadBeforeDelete,
    },
    mutation: {
      variables: deleteVariables,
      response: mutationResponse,
    },
    duplicateMutation: {
      variables: deleteVariables,
      response: duplicateMutationResponse,
    },
    downstreamRead: {
      variables: { id: sourceProductId },
      response: downstreamRead,
    },
    operationRead: {
      variables: { id: operationId },
      response: operationRead,
    },
    nodeRead: {
      variables: { id: operationId },
      response: nodeRead,
    },
    cleanup,
    upstreamCalls: [],
  };

  await writeFile(
    path.join(outputDir, 'product-delete-async-operation.json'),
    `${JSON.stringify(capture, null, 2)}\n`,
    'utf8',
  );

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: ['product-delete-async-operation.json'],
        sourceProductId,
        operationId,
      },
      null,
      2,
    ),
  );
} finally {
  if (!cleanup) {
    cleanup = await waitForAsyncDeleteCleanup(sourceProductId, operationId);
  }
}
