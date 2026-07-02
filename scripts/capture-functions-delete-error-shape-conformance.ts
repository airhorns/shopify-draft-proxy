/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'functions');
const outputPath = path.join(outputDir, 'functions-delete-error-shape.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type Capture = {
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

type JsonRecord = Record<string, unknown>;

async function capture(query: string, variables: Record<string, unknown>): Promise<Capture> {
  return {
    query,
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

function assertNoTopLevelErrors(captureResult: Capture, context: string): void {
  if (
    captureResult.response.status < 200 ||
    captureResult.response.status >= 300 ||
    captureResult.response.payload.errors
  ) {
    throw new Error(`${context} failed: ${JSON.stringify(captureResult.response, null, 2)}`);
  }
}

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current: unknown = value;
  for (const segment of pathSegments) {
    const record = readRecord(current);
    if (!record) {
      return undefined;
    }
    current = record[segment];
  }
  return current;
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

const cartTransformDeleteDocument = `mutation CartTransformDeleteMissing($id: ID!) {
  cartTransformDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const cartTransformCreateDocument = `mutation CartTransformCreateThenDelete($functionHandle: String!) {
  cartTransformCreate(functionHandle: $functionHandle) {
    cartTransform {
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

const cartTransformsReadDocument = `query CartTransformCreateThenDeleteRead {
  cartTransforms(first: 5) {
    nodes {
      id
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
}
`;

const cartTransformsCleanupReadDocument = `query CartTransformDeleteShapeCleanupRead {
  cartTransforms(first: 50) {
    nodes {
      id
      functionId
    }
  }
}
`;

const functionHydrateByHandleDocument = `query FunctionHydrateByHandle {
  shopifyFunctions(first: 100) {
    nodes {
      id
      title
      handle
      apiType
      description
      appKey
      app {
        __typename
        id
        title
        handle
        apiKey
      }
    }
  }
}
`;

const validationDeleteDocument = `mutation ValidationDeleteMissing($id: ID!) {
  validationDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const cartTransformFunctionHandle = 'conformance-cart-transform';

const cartTransformDeleteMissing = await capture(cartTransformDeleteDocument, {
  id: 'gid://shopify/CartTransform/999999999999',
});
const validationDeleteMissing = await capture(validationDeleteDocument, {
  id: 'gid://shopify/Validation/999999999999',
});
assertNoTopLevelErrors(cartTransformDeleteMissing, 'cartTransformDelete missing-id capture');
assertNoTopLevelErrors(validationDeleteMissing, 'validationDelete missing-id capture');

const functionHydrate = await capture(functionHydrateByHandleDocument, {
  handle: cartTransformFunctionHandle,
  apiType: 'CART_TRANSFORM',
});
assertNoTopLevelErrors(functionHydrate, 'FunctionHydrateByHandle cart-transform Function hydrate');
const cartTransformFunction = readArray(
  readPath(functionHydrate.response.payload, ['data', 'shopifyFunctions', 'nodes']),
)
  .map(readRecord)
  .find((node) => node?.['handle'] === cartTransformFunctionHandle);
if (!cartTransformFunction) {
  throw new Error(`Missing released cart-transform Function handle ${cartTransformFunctionHandle}`);
}

const cleanupRead = await capture(cartTransformsCleanupReadDocument, {});
assertNoTopLevelErrors(cleanupRead, 'cartTransforms cleanup read');
for (const node of readArray(readPath(cleanupRead.response.payload, ['data', 'cartTransforms', 'nodes']))) {
  const id = readString(readRecord(node)?.['id']);
  if (id) {
    const cleanupDelete = await capture(cartTransformDeleteDocument, { id });
    assertNoTopLevelErrors(cleanupDelete, `cartTransform cleanup delete ${id}`);
  }
}

let createdCartTransformId: string | null = null;
const cartTransformCreate = await capture(cartTransformCreateDocument, {
  functionHandle: cartTransformFunctionHandle,
});
assertNoTopLevelErrors(cartTransformCreate, 'cartTransformCreate live lifecycle capture');
const createPayload = readRecord(readPath(cartTransformCreate.response.payload, ['data', 'cartTransformCreate']));
const createUserErrors = readArray(createPayload?.['userErrors']);
createdCartTransformId = readString(readRecord(createPayload?.['cartTransform'])?.['id']);
if (createUserErrors.length > 0 || !createdCartTransformId) {
  throw new Error(`cartTransformCreate live lifecycle failed: ${JSON.stringify(createPayload, null, 2)}`);
}

let cartTransformDelete: Capture | null = null;
try {
  cartTransformDelete = await capture(cartTransformDeleteDocument, { id: createdCartTransformId });
  assertNoTopLevelErrors(cartTransformDelete, 'cartTransformDelete live lifecycle capture');
  createdCartTransformId = null;
} finally {
  if (createdCartTransformId) {
    const cleanupDelete = await capture(cartTransformDeleteDocument, { id: createdCartTransformId });
    if (
      cleanupDelete.response.status < 200 ||
      cleanupDelete.response.status >= 300 ||
      readRecord(cleanupDelete.response.payload)?.['errors']
    ) {
      console.error(`Cleanup failed for ${createdCartTransformId}: ${JSON.stringify(cleanupDelete, null, 2)}`);
    }
  }
}
const postDeleteRead = await capture(cartTransformsReadDocument, {});
assertNoTopLevelErrors(postDeleteRead, 'cartTransforms post-delete read');
if (!cartTransformDelete) {
  throw new Error('cartTransformDelete live lifecycle capture was not recorded.');
}

const fixture = {
  scenarioId: 'functions-delete-error-shape',
  capturedAt: new Date().toISOString(),
  source: 'live-shopify',
  storeDomain,
  apiVersion,
  summary:
    'Delete error-shape evidence for validationDelete and cartTransformDelete plus live cartTransformCreate/delete lifecycle.',
  conformanceApp: {
    cartTransformFunctionHandle,
    cartTransformFunction,
  },
  functionHydrate,
  cartTransformDeleteMissing,
  validationDeleteMissing,
  cartTransformCreateThenDelete: {
    create: {
      query: cartTransformCreate.query,
      variables: cartTransformCreate.variables,
      response: cartTransformCreate.response.payload,
    },
    delete: {
      query: cartTransformDelete.query,
      variables: cartTransformDelete.variables,
      response: cartTransformDelete.response.payload,
    },
    postDeleteRead: {
      query: postDeleteRead.query,
      variables: postDeleteRead.variables,
      response: postDeleteRead.response.payload,
    },
  },
  upstreamCalls: [
    {
      operationName: 'FunctionHydrateByHandle',
      variables: {
        handle: cartTransformFunctionHandle,
        apiType: 'CART_TRANSFORM',
      },
      query: functionHydrate.query,
      response: {
        status: functionHydrate.response.status,
        body: functionHydrate.response.payload,
      },
    },
  ],
  notes: {
    liveMissingDeleteEvidence:
      'cartTransformDelete and validationDelete missing-id userErrors are captured live from Shopify.',
    successPathEvidence:
      'The create/delete leg uses the released conformance cart-transform Function and records the live Shopify lifecycle plus the exact Function hydrate read used by proxy replay.',
  },
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
