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
const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
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

const productPayloadShopHydrateQuery = `#graphql
  query ProductPayloadShopHydrate {
    shop {
      id
      name
      myshopifyDomain
      url
      currencyCode
      primaryDomain {
        id
        host
        url
        sslEnabled
      }
    }
  }
`;

async function captureProductPayloadShopHydrateUpstreamCall() {
  const variables = {};
  const { status, payload } = await runGraphqlRequest(productPayloadShopHydrateQuery, variables);
  if (status < 200 || status >= 300 || payload.errors) {
    throw new Error(
      `Product payload shop hydrate cassette capture failed: ${JSON.stringify({ status, payload }, null, 2)}`,
    );
  }

  return {
    operationName: 'ProductPayloadShopHydrate',
    variables,
    query: productPayloadShopHydrateQuery,
    response: {
      status,
      body: payload,
    },
  };
}

const productDeleteAsyncMutation = `#graphql
  mutation ProductDeleteAsyncOperation($input: ProductDeleteInput!, $synchronous: Boolean!) {
    productDelete(input: $input, synchronous: $synchronous) {
      deletedProductId
      shop {
        id
        name
        myshopifyDomain
      }
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

const productDeleteRenamedVariableMutation = `#graphql
  mutation ProductDeleteAsyncRenamedVariable($input: ProductDeleteInput!, $runSynchronously: Boolean!) {
    productDelete(input: $input, synchronous: $runSynchronously) {
      deletedProductId
      shop {
        id
        name
        myshopifyDomain
      }
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

const productDeleteInlineFalseMutation = `#graphql
  mutation ProductDeleteAsyncInlineFalse($input: ProductDeleteInput!) {
    productDelete(input: $input, synchronous: false) {
      deletedProductId
      shop {
        id
        name
        myshopifyDomain
      }
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

async function createSourceProduct(title: string) {
  const sourceCreateVariables = {
    synchronous: true,
    input: {
      title,
      status: 'DRAFT',
    },
  };
  const sourceCreateResponse = (await runGraphql(productSetMutation, sourceCreateVariables)) as CapturedGraphqlResponse;
  expectNoUserErrors(
    'productSet async delete source create',
    responseData(sourceCreateResponse)['productSet']?.userErrors,
  );
  const sourceProductId = responseData(sourceCreateResponse)['productSet']?.product?.id ?? null;
  if (!sourceProductId) {
    throw new Error(`Async productDelete capture could not create disposable source product ${title}.`);
  }

  return {
    sourceCreateVariables,
    sourceCreateResponse,
    sourceProductId,
    sourceReadBeforeDelete: await readProduct(sourceProductId),
  };
}

async function captureAsyncDeleteCase(options: {
  label: string;
  mutation: string;
  title: string;
  variablesForProduct: (productId: string) => Record<string, unknown>;
  captureDuplicate?: boolean;
}) {
  let sourceProductId: string | null = null;
  let operationId: string | null = null;
  let cleanup: unknown | null = null;

  try {
    const source = await createSourceProduct(options.title);
    const productId = source.sourceProductId;
    sourceProductId = productId;
    const deleteVariables = options.variablesForProduct(productId);
    const mutationResponse = (await runGraphql(options.mutation, deleteVariables)) as CapturedGraphqlResponse;
    expectNoUserErrors(
      `${options.label} async productDelete mutation`,
      responseData(mutationResponse)['productDelete']?.userErrors,
    );
    operationId = responseData(mutationResponse)['productDelete']?.productDeleteOperation?.id ?? null;
    if (!operationId) {
      throw new Error(`${options.label} async productDelete capture did not return a ProductDeleteOperation id.`);
    }

    let duplicateMutation:
      | {
          variables: Record<string, unknown>;
          response: CapturedGraphqlResponse;
        }
      | undefined;
    if (options.captureDuplicate) {
      const duplicateMutationResponse = (await runGraphql(
        options.mutation,
        deleteVariables,
      )) as CapturedGraphqlResponse;
      expectPendingJobError(
        `${options.label} duplicate async productDelete mutation`,
        responseData(duplicateMutationResponse)['productDelete']?.userErrors,
      );
      duplicateMutation = {
        variables: deleteVariables,
        response: duplicateMutationResponse,
      };
    }

    const downstreamRead = await readProduct(productId);
    const operationRead = await readOperation(operationId);
    const nodeRead = (await runGraphql(nodeReadQuery, { id: operationId })) as CapturedGraphqlResponse;
    cleanup = await waitForAsyncDeleteCleanup(productId, operationId);

    return {
      setup: {
        sourceCreate: {
          variables: source.sourceCreateVariables,
          response: source.sourceCreateResponse,
        },
        sourceProductId,
        sourceReadBeforeDelete: source.sourceReadBeforeDelete,
      },
      mutation: {
        variables: deleteVariables,
        response: mutationResponse,
      },
      ...(duplicateMutation ? { duplicateMutation } : {}),
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
    };
  } finally {
    if (!cleanup) {
      cleanup = await waitForAsyncDeleteCleanup(sourceProductId, operationId);
    }
  }
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const shopHydrateCalls = [
  await captureProductPayloadShopHydrateUpstreamCall(),
  await captureProductPayloadShopHydrateUpstreamCall(),
];
const canonical = await captureAsyncDeleteCase({
  label: 'canonical-variable',
  mutation: productDeleteAsyncMutation,
  title: `Async Delete Source ${runId}`,
  variablesForProduct: (productId) => ({
    input: { id: productId },
    synchronous: false,
  }),
  captureDuplicate: true,
});
const renamedVariable = await captureAsyncDeleteCase({
  label: 'renamed-variable',
  mutation: productDeleteRenamedVariableMutation,
  title: `Async Delete Renamed Variable ${runId}`,
  variablesForProduct: (productId) => ({
    input: { id: productId },
    runSynchronously: false,
  }),
});
const inlineFalse = await captureAsyncDeleteCase({
  label: 'inline-false',
  mutation: productDeleteInlineFalseMutation,
  title: `Async Delete Inline False ${runId}`,
  variablesForProduct: (productId) => ({
    input: { id: productId },
  }),
});

const capture = {
  storeDomain,
  apiVersion,
  setup: canonical.setup,
  mutation: canonical.mutation,
  duplicateMutation: canonical.duplicateMutation,
  downstreamRead: canonical.downstreamRead,
  operationRead: canonical.operationRead,
  nodeRead: canonical.nodeRead,
  cleanup: canonical.cleanup,
  renamedVariable,
  inlineFalse,
  upstreamCalls: shopHydrateCalls,
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
      canonicalProductId: canonical.setup.sourceProductId,
      renamedVariableProductId: renamedVariable.setup.sourceProductId,
      inlineFalseProductId: inlineFalse.setup.sourceProductId,
    },
    null,
    2,
  ),
);
