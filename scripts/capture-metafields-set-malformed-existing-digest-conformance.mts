/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: {
    status: number;
    body: ConformanceGraphqlPayload;
  };
};

const requestPaths = {
  productCreate: 'config/parity-requests/products/metafieldsSet-malformed-existing-product-create.graphql',
  metafieldsSet: 'config/parity-requests/products/metafieldsSet-malformed-existing-mutation.graphql',
  downstreamRead: 'config/parity-requests/products/metafieldsSet-malformed-existing-downstream-read.graphql',
};

const documents = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([key, filePath]) => [key, await readFile(filePath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const productDeleteMutation = `#graphql
mutation MetafieldsSetMalformedExistingProductDelete($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}
`;

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
const outputPath = path.join(outputDir, 'metafields-set-malformed-existing-digest.json');

function readPath(value: unknown, segments: Array<string | number>): unknown {
  let current = value;
  for (const segment of segments) {
    if (Array.isArray(current) && typeof segment === 'number') {
      current = current[segment];
      continue;
    }
    if (typeof current !== 'object' || current === null || typeof segment !== 'string') return undefined;
    current = (current as JsonRecord)[segment];
  }
  return current;
}

function requiredString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} was not a non-empty string: ${JSON.stringify(value)}`);
  }
  return value;
}

function expectEmptyUserErrors(body: unknown, root: string): void {
  const userErrors = readPath(body, ['data', root, 'userErrors']);
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${root} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function expectMetafieldValue(body: unknown, expected: string, label: string): void {
  const value = readPath(body, ['data', 'metafieldsSet', 'metafields', 0, 'value']);
  if (value !== expected) {
    throw new Error(`${label} returned ${JSON.stringify(value)} instead of ${JSON.stringify(expected)}`);
  }
}

async function captureGraphql(query: string, variables: JsonRecord): Promise<GraphqlCapture> {
  const { status, payload } = await runGraphqlRequest(query, variables);
  if (status < 200 || status >= 300 || payload.errors) {
    throw new Error(`GraphQL capture failed: ${JSON.stringify({ status, payload }, null, 2)}`);
  }
  return {
    request: { query, variables },
    response: { status, body: payload },
  };
}

const runId = Date.now().toString(36);
const namespace = `malformed_digest_${runId}`;
const key = 'existing_row';
const initialValue = 'Before malformed digest';
const changedValue = 'After malformed digest';
const malformedDigest = 'not-a-hex-digest';
let productId: string | null = null;
let workflow: {
  setup: {
    productCreate: GraphqlCapture;
    initialSet: GraphqlCapture;
  };
  malformedExistingSet: GraphqlCapture;
  malformedChangingSet: GraphqlCapture;
  downstreamRead: GraphqlCapture;
} | null = null;
let cleanup: GraphqlCapture | null = null;

try {
  const productCreate = await captureGraphql(documents.productCreate, {
    product: { title: `Malformed compareDigest ${runId}` },
  });
  expectEmptyUserErrors(productCreate.response.body, 'productCreate');
  productId = requiredString(
    readPath(productCreate.response.body, ['data', 'productCreate', 'product', 'id']),
    'productCreate.product.id',
  );

  const initialSet = await captureGraphql(documents.metafieldsSet, {
    metafields: [
      {
        ownerId: productId,
        namespace,
        key,
        type: 'single_line_text_field',
        value: initialValue,
      },
    ],
  });
  expectEmptyUserErrors(initialSet.response.body, 'metafieldsSet');
  expectMetafieldValue(initialSet.response.body, initialValue, 'Initial metafieldsSet');

  const malformedExistingSet = await captureGraphql(documents.metafieldsSet, {
    metafields: [
      {
        ownerId: productId,
        namespace,
        key,
        type: 'single_line_text_field',
        value: initialValue,
        compareDigest: malformedDigest,
      },
    ],
  });
  expectEmptyUserErrors(malformedExistingSet.response.body, 'metafieldsSet');
  expectMetafieldValue(malformedExistingSet.response.body, initialValue, 'Malformed-digest metafieldsSet');

  const malformedChangingSet = await captureGraphql(documents.metafieldsSet, {
    metafields: [
      {
        ownerId: productId,
        namespace,
        key,
        type: 'single_line_text_field',
        value: changedValue,
        compareDigest: malformedDigest,
      },
    ],
  });
  const changedMetafields = readPath(malformedChangingSet.response.body, ['data', 'metafieldsSet', 'metafields']);
  const changedErrors = readPath(malformedChangingSet.response.body, ['data', 'metafieldsSet', 'userErrors']);
  if (
    !Array.isArray(changedMetafields) ||
    changedMetafields.length !== 0 ||
    !Array.isArray(changedErrors) ||
    changedErrors.length !== 1 ||
    readPath(changedErrors, [0, 'code']) !== 'STALE_OBJECT' ||
    JSON.stringify(readPath(changedErrors, [0, 'field'])) !== JSON.stringify(['metafields', '0'])
  ) {
    throw new Error(
      `Changed-value malformed digest returned an unexpected payload: ${JSON.stringify(malformedChangingSet.response.body)}`,
    );
  }

  const downstreamRead = await captureGraphql(documents.downstreamRead, { id: productId, namespace, key });
  const downstreamValue = readPath(downstreamRead.response.body, ['data', 'product', 'malformedDigest', 'value']);
  if (downstreamValue !== initialValue) {
    throw new Error(
      `Downstream metafield value was ${JSON.stringify(downstreamValue)} instead of ${JSON.stringify(initialValue)}`,
    );
  }

  workflow = {
    setup: { productCreate, initialSet },
    malformedExistingSet,
    malformedChangingSet,
    downstreamRead,
  };
} finally {
  if (productId !== null) {
    cleanup = await captureGraphql(productDeleteMutation, { input: { id: productId } });
    expectEmptyUserErrors(cleanup.response.body, 'productDelete');
  }
}

if (workflow === null || cleanup === null) {
  throw new Error('Malformed compareDigest capture did not complete its setup, behavior, readback, and cleanup flow.');
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      ...workflow,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
