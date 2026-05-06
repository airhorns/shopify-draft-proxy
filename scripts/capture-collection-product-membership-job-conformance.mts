import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlPayload = Record<string, unknown>;

type SeedProduct = {
  id: string;
  title: string;
  handle: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productsDir = path.join('config', 'parity-requests', 'products');
const specsDir = path.join('config', 'parity-specs', 'products');
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');

const createDocumentPath = path.join(productsDir, 'collection-product-membership-job-create.graphql');
const addV2DocumentPath = path.join(productsDir, 'collection-product-membership-job-add-v2.graphql');
const removeDocumentPath = path.join(productsDir, 'collection-product-membership-job-remove.graphql');
const jobReadDocumentPath = path.join(productsDir, 'collection-product-membership-job-read.graphql');
const specPath = path.join(specsDir, 'collection-product-membership-job-parity.json');
const fixturePath = path.join(fixtureDir, 'collection-product-membership-job-parity.json');

const seedProductsQuery = `#graphql
query CollectionMembershipJobSeedProduct {
  products(first: 1, sortKey: UPDATED_AT, reverse: true) {
    nodes {
      id
      title
      handle
    }
  }
}
`;

const createDocument = `mutation CollectionMembershipJobCollectionCreate($input: CollectionInput!) {
  collectionCreate(input: $input) {
    collection {
      id
      title
      handle
      sortOrder
      ruleSet {
        appliedDisjunctively
        rules {
          column
          relation
          condition
        }
      }
      productsCount {
        count
        precision
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const addV2Document = `mutation CollectionMembershipJobAddProductsV2($id: ID!, $productIds: [ID!]!) {
  collectionAddProductsV2(id: $id, productIds: $productIds) {
    job {
      id
      done
      query {
        __typename
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const removeDocument = `mutation CollectionMembershipJobRemoveProducts($id: ID!, $productIds: [ID!]!) {
  collectionRemoveProducts(id: $id, productIds: $productIds) {
    job {
      id
      done
      query {
        __typename
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const jobReadDocument = `query CollectionMembershipJobRead($id: ID!) {
  job(id: $id) {
    __typename
    id
    done
    query {
      __typename
    }
  }
}
`;

const deleteDocument = `#graphql
mutation CollectionMembershipJobCleanup($input: CollectionDeleteInput!) {
  collectionDelete(input: $input) {
    deletedCollectionId
    userErrors {
      field
      message
    }
  }
}
`;

async function runGraphqlPayload(query: string, variables?: Record<string, unknown>): Promise<GraphqlPayload> {
  try {
    return await runGraphql(query, variables);
  } catch (error) {
    const payload = (error as { result?: { payload?: GraphqlPayload } }).result?.payload;
    if (payload) {
      return payload;
    }
    throw error;
  }
}

function firstSeedProduct(payload: GraphqlPayload): SeedProduct {
  const product = (
    ((payload['data'] as Record<string, unknown> | undefined)?.['products'] as Record<string, unknown> | undefined)?.[
      'nodes'
    ] as Array<Record<string, unknown>> | undefined
  )?.[0];
  if (
    typeof product?.['id'] !== 'string' ||
    typeof product['title'] !== 'string' ||
    typeof product['handle'] !== 'string'
  ) {
    throw new Error('Need at least one live product to capture collection product membership job parity.');
  }
  return {
    id: product['id'],
    title: product['title'],
    handle: product['handle'],
  };
}

function collectionId(payload: GraphqlPayload): string | null {
  const id = (
    (
      (payload['data'] as Record<string, unknown> | undefined)?.['collectionCreate'] as
        | Record<string, unknown>
        | undefined
    )?.['collection'] as Record<string, unknown> | undefined
  )?.['id'];
  return typeof id === 'string' ? id : null;
}

function jobId(payload: GraphqlPayload, root: 'collectionAddProductsV2' | 'collectionRemoveProducts'): string {
  const id = (
    ((payload['data'] as Record<string, unknown> | undefined)?.[root] as Record<string, unknown> | undefined)?.[
      'job'
    ] as Record<string, unknown> | undefined
  )?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`${root} did not return a job id: ${JSON.stringify(payload)}`);
  }
  return id;
}

async function cleanupCollection(id: string | null): Promise<void> {
  if (!id) {
    return;
  }
  try {
    await runGraphql(deleteDocument, { input: { id } });
  } catch {
    // Best-effort cleanup only; the capture should preserve the original response.
  }
}

function repeatedProductIds(productId: string, count: number): string[] {
  return Array.from({ length: count }, () => productId);
}

await mkdir(productsDir, { recursive: true });
await mkdir(specsDir, { recursive: true });
await mkdir(fixtureDir, { recursive: true });

const runId = `${Date.now()}`;
const seedProduct = firstSeedProduct(await runGraphql(seedProductsQuery));
const missingProductId = `gid://shopify/Product/999999999${runId}`;
const createdIds: string[] = [];

try {
  const smartCreateVariables = {
    input: {
      title: `Collection Membership Job Smart ${runId}`,
      ruleSet: {
        appliedDisjunctively: false,
        rules: [{ column: 'TITLE', relation: 'CONTAINS', condition: `membership-job-${runId}` }],
      },
    },
  };
  const customCreateVariables = {
    input: {
      title: `Collection Membership Job Custom ${runId}`,
      sortOrder: 'MANUAL',
    },
  };

  const smartCreate = await runGraphqlPayload(createDocument, smartCreateVariables);
  const smartId = collectionId(smartCreate);
  if (!smartId) {
    throw new Error('Smart collection create did not return a collection id.');
  }
  createdIds.push(smartId);

  const customCreate = await runGraphqlPayload(createDocument, customCreateVariables);
  const customId = collectionId(customCreate);
  if (!customId) {
    throw new Error('Custom collection create did not return a collection id.');
  }
  createdIds.push(customId);

  const smartVariables = { id: smartId, productIds: [seedProduct.id] };
  const unknownVariables = { id: customId, productIds: [missingProductId] };
  const tooManyVariables = { id: customId, productIds: repeatedProductIds(seedProduct.id, 251) };
  const successVariables = { id: customId, productIds: [seedProduct.id] };

  const smartAddV2 = await runGraphqlPayload(addV2Document, smartVariables);
  const smartRemove = await runGraphqlPayload(removeDocument, smartVariables);
  const unknownAddV2 = await runGraphqlPayload(addV2Document, unknownVariables);
  const tooManyAddV2 = await runGraphqlPayload(addV2Document, tooManyVariables);
  const successAddV2 = await runGraphqlPayload(addV2Document, successVariables);
  const successAddJobReadVariables = { id: jobId(successAddV2, 'collectionAddProductsV2') };
  const successAddJobRead = await runGraphqlPayload(jobReadDocument, successAddJobReadVariables);
  const unknownRemove = await runGraphqlPayload(removeDocument, unknownVariables);
  const tooManyRemove = await runGraphqlPayload(removeDocument, tooManyVariables);
  const successRemove = await runGraphqlPayload(removeDocument, successVariables);
  const successRemoveJobReadVariables = { id: jobId(successRemove, 'collectionRemoveProducts') };
  const successRemoveJobRead = await runGraphqlPayload(jobReadDocument, successRemoveJobReadVariables);

  const fixture = {
    storeDomain,
    apiVersion,
    seedProduct,
    missingProductId,
    smartCreate: { variables: smartCreateVariables, response: smartCreate },
    customCreate: { variables: customCreateVariables, response: customCreate },
    smartAddV2: { variables: smartVariables, response: smartAddV2 },
    smartRemove: { variables: smartVariables, response: smartRemove },
    unknownAddV2: { variables: unknownVariables, response: unknownAddV2 },
    tooManyAddV2: { variables: tooManyVariables, response: tooManyAddV2 },
    successAddV2: { variables: successVariables, response: successAddV2 },
    successAddJobRead: { variables: successAddJobReadVariables, response: successAddJobRead },
    unknownRemove: { variables: unknownVariables, response: unknownRemove },
    tooManyRemove: { variables: tooManyVariables, response: tooManyRemove },
    successRemove: { variables: successVariables, response: successRemove },
    successRemoveJobRead: { variables: successRemoveJobReadVariables, response: successRemoveJobRead },
    upstreamCalls: [],
  };

  const spec = {
    scenarioId: 'collection-product-membership-job-parity',
    operationNames: ['collectionCreate', 'collectionAddProductsV2', 'collectionRemoveProducts'],
    scenarioStatus: 'captured',
    assertionKinds: [
      'payload-shape',
      'user-errors-parity',
      'smart-collection-guards',
      'async-job-readback',
      'input-cap-validation',
    ],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: createDocumentPath,
      variablesCapturePath: '$.smartCreate.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Executable parity for collectionAddProductsV2 and collectionRemoveProducts smart-collection guards, async Job payload/readback, unknown productIds acceptance, and the 250-item productIds cap. Live Admin GraphQL returns top-level MAX_INPUT_SIZE_EXCEEDED for 251 productIds and accepts unknown product IDs asynchronously with an empty userErrors array.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [
        {
          path: '$.collectionCreate.collection.id',
          matcher: 'shopify-gid:Collection',
          reason: 'Shopify and the local parity harness allocate collection identifiers independently.',
        },
        {
          path: '$.collectionAddProductsV2.job.id',
          matcher: 'shopify-gid:Job',
          reason: 'Shopify and the local parity harness allocate async job identifiers independently.',
        },
        {
          path: '$.collectionRemoveProducts.job.id',
          matcher: 'shopify-gid:Job',
          reason: 'Shopify and the local parity harness allocate async job identifiers independently.',
        },
        {
          path: '$.job.id',
          matcher: 'shopify-gid:Job',
          reason: 'The job readback targets the job id allocated by the local parity run.',
        },
      ],
      targets: [
        {
          name: 'smart-create',
          capturePath: '$.smartCreate.response.data',
          proxyPath: '$.data',
        },
        {
          name: 'smart-add-v2-user-error',
          capturePath: '$.smartAddV2.response.data',
          proxyRequest: {
            documentPath: addV2DocumentPath,
            apiVersion,
            variables: {
              id: {
                fromProxyResponse: 'smart-create',
                path: '$.data.collectionCreate.collection.id',
              },
              productIds: [{ fromCapturePath: '$.seedProduct.id' }],
            },
          },
          proxyPath: '$.data',
        },
        {
          name: 'smart-remove-user-error',
          capturePath: '$.smartRemove.response.data',
          proxyRequest: {
            documentPath: removeDocumentPath,
            apiVersion,
            variables: {
              id: {
                fromProxyResponse: 'smart-create',
                path: '$.data.collectionCreate.collection.id',
              },
              productIds: [{ fromCapturePath: '$.seedProduct.id' }],
            },
          },
          proxyPath: '$.data',
        },
        {
          name: 'custom-create',
          capturePath: '$.customCreate.response.data',
          proxyRequest: {
            documentPath: createDocumentPath,
            variablesCapturePath: '$.customCreate.variables',
            apiVersion,
          },
          proxyPath: '$.data',
        },
        {
          name: 'unknown-add-v2-accepted-job',
          capturePath: '$.unknownAddV2.response.data',
          proxyRequest: {
            documentPath: addV2DocumentPath,
            apiVersion,
            variables: {
              id: {
                fromProxyResponse: 'custom-create',
                path: '$.data.collectionCreate.collection.id',
              },
              productIds: [{ fromCapturePath: '$.missingProductId' }],
            },
          },
          proxyPath: '$.data',
        },
        {
          name: 'too-many-add-v2-top-level-error',
          capturePath: '$.tooManyAddV2.response.errors',
          proxyRequest: {
            documentPath: addV2DocumentPath,
            variablesCapturePath: '$.tooManyAddV2.variables',
            apiVersion,
          },
          proxyPath: '$.errors',
        },
        {
          name: 'success-add-v2-job',
          capturePath: '$.successAddV2.response.data',
          proxyRequest: {
            documentPath: addV2DocumentPath,
            apiVersion,
            variables: {
              id: {
                fromProxyResponse: 'custom-create',
                path: '$.data.collectionCreate.collection.id',
              },
              productIds: [{ fromCapturePath: '$.seedProduct.id' }],
            },
          },
          proxyPath: '$.data',
        },
        {
          name: 'success-add-v2-job-read',
          capturePath: '$.successAddJobRead.response.data',
          proxyRequest: {
            documentPath: jobReadDocumentPath,
            apiVersion,
            variables: {
              id: {
                fromProxyResponse: 'success-add-v2-job',
                path: '$.data.collectionAddProductsV2.job.id',
              },
            },
          },
          proxyPath: '$.data',
        },
        {
          name: 'unknown-remove-accepted-job',
          capturePath: '$.unknownRemove.response.data',
          proxyRequest: {
            documentPath: removeDocumentPath,
            apiVersion,
            variables: {
              id: {
                fromProxyResponse: 'custom-create',
                path: '$.data.collectionCreate.collection.id',
              },
              productIds: [{ fromCapturePath: '$.missingProductId' }],
            },
          },
          proxyPath: '$.data',
        },
        {
          name: 'too-many-remove-top-level-error',
          capturePath: '$.tooManyRemove.response.errors',
          proxyRequest: {
            documentPath: removeDocumentPath,
            variablesCapturePath: '$.tooManyRemove.variables',
            apiVersion,
          },
          proxyPath: '$.errors',
        },
        {
          name: 'success-remove-job',
          capturePath: '$.successRemove.response.data',
          proxyRequest: {
            documentPath: removeDocumentPath,
            apiVersion,
            variables: {
              id: {
                fromProxyResponse: 'custom-create',
                path: '$.data.collectionCreate.collection.id',
              },
              productIds: [{ fromCapturePath: '$.seedProduct.id' }],
            },
          },
          proxyPath: '$.data',
        },
        {
          name: 'success-remove-job-read',
          capturePath: '$.successRemoveJobRead.response.data',
          proxyRequest: {
            documentPath: jobReadDocumentPath,
            apiVersion,
            variables: {
              id: {
                fromProxyResponse: 'success-remove-job',
                path: '$.data.collectionRemoveProducts.job.id',
              },
            },
          },
          proxyPath: '$.data',
        },
      ],
    },
  };

  await writeFile(createDocumentPath, createDocument, 'utf8');
  await writeFile(addV2DocumentPath, addV2Document, 'utf8');
  await writeFile(removeDocumentPath, removeDocument, 'utf8');
  await writeFile(jobReadDocumentPath, jobReadDocument, 'utf8');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
  console.log(
    JSON.stringify(
      {
        ok: true,
        fixturePath,
        specPath,
        requestFiles: [createDocumentPath, addV2DocumentPath, removeDocumentPath, jobReadDocumentPath],
      },
      null,
      2,
    ),
  );
} finally {
  await Promise.allSettled(createdIds.map(cleanupCollection));
}
