/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonObject = Record<string, unknown>;

type Capture = {
  query: string;
  variables: JsonObject;
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafieldsSet-custom-namespace-typed-keys.json');
const setDocumentPath = path.join(
  'config',
  'parity-requests',
  'metafields',
  'metafieldsSet-custom-namespace-typed-keys.graphql',
);
const readDocumentPath = path.join(
  'config',
  'parity-requests',
  'metafields',
  'metafieldsSet-custom-namespace-typed-keys-read.graphql',
);

const setDocument = await readFile(setDocumentPath, 'utf8');
const readDocument = await readFile(readDocumentPath, 'utf8');
const runId = `${Date.now()}`;

const productCreateMutation = `#graphql
  mutation CustomNamespaceTypedKeysProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product { id title }
      userErrors { field message }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation CustomNamespaceTypedKeysProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

function asObject(value: unknown, context: string): JsonObject {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonObject;
}

function getPath(value: unknown, pathParts: string[], context: string): unknown {
  let cursor = value;
  for (const part of pathParts) {
    if (!cursor || typeof cursor !== 'object' || Array.isArray(cursor)) {
      throw new Error(`${context} missing ${pathParts.join('.')}: ${JSON.stringify(value)}`);
    }
    cursor = (cursor as JsonObject)[part];
  }
  return cursor;
}

function requireNoUserErrors(payload: unknown, pathParts: string[], context: string): void {
  const userErrors = getPath(payload, pathParts, context);
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors)}`);
}

async function capture(query: string, variables: JsonObject): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  return {
    query,
    variables,
    status: result.status,
    response: result.payload,
  };
}

async function runRequired(query: string, variables: JsonObject, context: string): Promise<ConformanceGraphqlResult> {
  const result = await runGraphqlRaw(query, variables);
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result.payload)}`);
  }
  return result;
}

let productId: string | null = null;
let fixture: {
  storeDomain: string;
  apiVersion: string;
  capturedAt: string;
  setupProduct: Capture;
  set: Capture;
  readAfterSet: Capture;
  cleanup: Capture[];
  upstreamCalls: [];
} | null = null;
const cleanup: Capture[] = [];

try {
  const setupVariables = {
    product: {
      title: `Hermes Custom Typed Metafields ${runId}`,
    },
  };
  const create = await runRequired(productCreateMutation, setupVariables, 'productCreate setup');
  const createPayload = asObject(create.payload, 'productCreate payload');
  requireNoUserErrors(createPayload, ['data', 'productCreate', 'userErrors'], 'productCreate setup');
  const createdProductId = getPath(createPayload, ['data', 'productCreate', 'product', 'id'], 'productCreate setup');
  if (typeof createdProductId !== 'string') {
    throw new Error(`productCreate setup did not return a product id: ${JSON.stringify(createPayload)}`);
  }
  productId = createdProductId;

  const setVariables = {
    metafields: [
      {
        ownerId: productId,
        namespace: 'custom',
        key: 'json',
        type: 'json',
        value: `{"a":1,"run":"${runId}"}`,
      },
      {
        ownerId: productId,
        namespace: 'custom',
        key: 'rating',
        type: 'rating',
        value: '{"scale_min":"1.0","scale_max":"5.0","value":"4.5"}',
      },
      {
        ownerId: productId,
        namespace: 'custom',
        key: 'money',
        type: 'money',
        value: '{"amount":"12.34","currency_code":"CAD"}',
      },
    ],
  };
  const readVariables = { id: productId };

  const set = await capture(setDocument, setVariables);
  requireNoUserErrors(set.response, ['data', 'metafieldsSet', 'userErrors'], 'metafieldsSet');
  const readAfterSet = await capture(readDocument, readVariables);

  fixture = {
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    setupProduct: {
      query: productCreateMutation,
      variables: setupVariables,
      status: create.status,
      response: create.payload,
    },
    set,
    readAfterSet,
    cleanup,
    upstreamCalls: [],
  };
} finally {
  if (productId) {
    cleanup.push(await capture(productDeleteMutation, { input: { id: productId } }));
  }
}

if (!fixture || !productId) {
  throw new Error('Capture did not produce a fixture.');
}

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({ ok: true, outputPath, productId, runId }, null, 2));
