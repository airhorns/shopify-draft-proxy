/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
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
  mutation ProductDuplicateAsyncSourceCreate($input: ProductSetInput!, $synchronous: Boolean!) {
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

const sourceReadQuery = `#graphql
  query ProductDuplicateAsyncSourceRead($id: ID!) {
    product(id: $id) {
      id
      title
      handle
      status
    }
  }
`;

const productDuplicateMutation = `#graphql
  mutation ProductDuplicateAsync($productId: ID!, $newTitle: String!) {
    productDuplicate(productId: $productId, newTitle: $newTitle, synchronous: false) {
      newProduct {
        id
        title
        handle
        status
      }
      productDuplicateOperation {
        id
        status
        product {
          id
          title
          handle
        }
        newProduct {
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
      userErrors {
        field
        message
      }
    }
  }
`;

const operationReadQuery = `#graphql
  query ProductDuplicateAsyncOperationRead($id: ID!) {
    productOperation(id: $id) {
      __typename
      status
      product {
        id
        title
        handle
      }
      ... on ProductDuplicateOperation {
        id
        newProduct {
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
  }
`;

const productReadQuery = `#graphql
  query ProductDuplicateAsyncProductRead($id: ID!) {
    product(id: $id) {
      id
      title
      handle
      status
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation ProductDuplicateAsyncDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
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

async function waitForProductDuplicateOperation(operationId: string): Promise<CapturedGraphqlResponse> {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const response = (await runGraphql(operationReadQuery, { id: operationId })) as CapturedGraphqlResponse;
    const status = responseData(response)['productOperation']?.status;
    if (status === 'COMPLETE') {
      return response;
    }

    await sleep(2000);
  }

  throw new Error(`Timed out waiting for ProductDuplicateOperation ${operationId} to complete.`);
}

async function deleteProduct(productId: string | null): Promise<unknown | null> {
  if (!productId) {
    return null;
  }

  try {
    return await runGraphql(productDeleteMutation, { input: { id: productId } });
  } catch (error) {
    return {
      cleanupError: error instanceof Error ? error.message : String(error),
    };
  }
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
let sourceProductId: string | null = null;
let duplicateProductId: string | null = null;
let sourceDeleteResponse: unknown | null = null;
let duplicateDeleteResponse: unknown | null = null;

try {
  const sourceCreateVariables = {
    synchronous: true,
    input: {
      title: `HAR407 Async Source ${runId}`,
      status: 'DRAFT',
    },
  };
  const sourceCreateResponse = (await runGraphql(productSetMutation, sourceCreateVariables)) as CapturedGraphqlResponse;
  expectNoUserErrors(
    'productSet async duplicate source create',
    responseData(sourceCreateResponse)['productSet']?.userErrors,
  );
  sourceProductId = responseData(sourceCreateResponse)['productSet']?.product?.id ?? null;
  if (!sourceProductId) {
    throw new Error('Async productDuplicate capture could not create a disposable source product.');
  }

  const sourceReadBeforeDuplicate = (await runGraphql(sourceReadQuery, {
    id: sourceProductId,
  })) as CapturedGraphqlResponse;
  const successVariables = {
    productId: sourceProductId,
    newTitle: `HAR407 Async Copy ${runId}`,
  };
  const successMutationResponse = (await runGraphql(
    productDuplicateMutation,
    successVariables,
  )) as CapturedGraphqlResponse;
  expectNoUserErrors(
    'async productDuplicate success mutation',
    responseData(successMutationResponse)['productDuplicate']?.userErrors,
  );
  const successOperationId =
    responseData(successMutationResponse)['productDuplicate']?.productDuplicateOperation?.id ?? null;
  if (!successOperationId) {
    throw new Error('Async productDuplicate success capture did not return a ProductDuplicateOperation id.');
  }

  const successOperationRead = await waitForProductDuplicateOperation(successOperationId);
  duplicateProductId = responseData(successOperationRead)['productOperation']?.newProduct?.id ?? null;
  if (!duplicateProductId) {
    throw new Error('Async productDuplicate operation read did not expose a duplicated product id.');
  }
  const successDownstreamRead = (await runGraphql(productReadQuery, {
    id: duplicateProductId,
  })) as CapturedGraphqlResponse;

  const missingVariables = {
    productId: 'gid://shopify/Product/999999999999999999',
    newTitle: `HAR407 Async Missing ${runId}`,
  };
  const missingMutationResponse = (await runGraphql(
    productDuplicateMutation,
    missingVariables,
  )) as CapturedGraphqlResponse;
  expectNoUserErrors(
    'async productDuplicate missing mutation',
    responseData(missingMutationResponse)['productDuplicate']?.userErrors,
  );
  const missingOperationId =
    responseData(missingMutationResponse)['productDuplicate']?.productDuplicateOperation?.id ?? null;
  if (!missingOperationId) {
    throw new Error('Async productDuplicate missing capture did not return a ProductDuplicateOperation id.');
  }

  const missingOperationRead = await waitForProductDuplicateOperation(missingOperationId);

  duplicateDeleteResponse = await deleteProduct(duplicateProductId);
  sourceDeleteResponse = await deleteProduct(sourceProductId);

  const successCapture = {
    setup: {
      sourceCreate: {
        variables: sourceCreateVariables,
        response: sourceCreateResponse,
      },
      sourceProductId,
      sourceReadBeforeDuplicate,
    },
    mutation: {
      variables: successVariables,
      response: successMutationResponse,
    },
    operationRead: {
      variables: { id: successOperationId },
      response: successOperationRead,
    },
    downstreamRead: {
      variables: { id: duplicateProductId },
      response: successDownstreamRead,
    },
    cleanup: {
      duplicateDelete: duplicateDeleteResponse,
      sourceDelete: sourceDeleteResponse,
    },
  };

  const missingCapture = {
    mutation: {
      variables: missingVariables,
      response: missingMutationResponse,
    },
    operationRead: {
      variables: { id: missingOperationId },
      response: missingOperationRead,
    },
  };

  await writeFile(
    path.join(outputDir, 'product-duplicate-async-success.json'),
    `${JSON.stringify(successCapture, null, 2)}\n`,
    'utf8',
  );
  await writeFile(
    path.join(outputDir, 'product-duplicate-async-missing.json'),
    `${JSON.stringify(missingCapture, null, 2)}\n`,
    'utf8',
  );

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: ['product-duplicate-async-success.json', 'product-duplicate-async-missing.json'],
        sourceProductId,
        duplicateProductId,
      },
      null,
      2,
    ),
  );
} finally {
  if (!duplicateDeleteResponse) {
    await deleteProduct(duplicateProductId);
  }
  if (!sourceDeleteResponse) {
    await deleteProduct(sourceProductId);
  }
}
