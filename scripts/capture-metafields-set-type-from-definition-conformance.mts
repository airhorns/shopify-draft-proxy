/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedInteraction = {
  request: {
    documentPath?: string;
    query?: string;
    variables: Record<string, unknown>;
  };
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

const requestDir = path.join('config', 'parity-requests', 'metafields');
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const createDefinitionDocumentPath = path.join(requestDir, 'metafield-definition-lifecycle-create.graphql');
const setWithoutTypeDocumentPath = path.join(requestDir, 'metafieldsSet-type-from-definition.graphql');
const readAfterSetDocumentPath = path.join(requestDir, 'metafieldsSet-type-from-definition-read.graphql');
const outputPath = path.join(outputDir, 'metafieldsSet-type-from-definition.json');

const productCreateMutation = `#graphql
mutation MetafieldsSetTypeFromDefinitionProductCreate($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product { id title }
    userErrors { field message }
  }
}
`;

const productDeleteMutation = `#graphql
mutation MetafieldsSetTypeFromDefinitionProductDelete($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors { field message }
  }
}
`;

const deleteDefinitionMutation = `#graphql
mutation MetafieldsSetTypeFromDefinitionDefinitionDelete($id: ID!) {
  metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
    deletedDefinitionId
    userErrors { field message code }
  }
}
`;

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, parts: string[]): unknown {
  let cursor: unknown = value;
  for (const part of parts) {
    if (Array.isArray(cursor)) {
      cursor = cursor[Number(part)];
    } else {
      cursor = readObject(cursor)?.[part];
    }
  }
  return cursor;
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} was not returned: ${JSON.stringify(value)}`);
  }
  return value;
}

function requireNoUserErrors(payload: unknown, parts: string[], label: string): void {
  const userErrors = readPath(payload, parts);
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function requirePathEquals(payload: unknown, parts: string[], expected: unknown, label: string): void {
  const actual = readPath(payload, parts);
  if (actual !== expected) {
    throw new Error(`${label} expected ${JSON.stringify(expected)} but got ${JSON.stringify(actual)}`);
  }
}

async function captureQuery(
  label: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedInteraction> {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

async function captureDocument(
  label: string,
  documentPath: string,
  variables: Record<string, unknown>,
): Promise<CapturedInteraction> {
  const query = await readFile(documentPath, 'utf8');
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    request: { documentPath, variables },
    status: result.status,
    response: result.payload,
  };
}

function requireCapture(value: CapturedInteraction | null, label: string): CapturedInteraction {
  if (!value) throw new Error(`${label} was not captured`);
  return value;
}

const suffix = Date.now().toString(36);
const namespace = `type_from_definition_${suffix}`;
const key = 'specs';
const metafieldType = 'multi_line_text_field';
const metafieldValue = 'hello world';
let productId: string | null = null;
let definitionId: string | null = null;
const cleanup: CapturedInteraction[] = [];
let productCreate: CapturedInteraction | null = null;
let createDefinition: CapturedInteraction | null = null;
let setWithoutType: CapturedInteraction | null = null;
let readAfterSet: CapturedInteraction | null = null;

try {
  productCreate = await captureQuery('productCreate setup', productCreateMutation, {
    product: { title: `metafieldsSet type from definition ${suffix}` },
  });
  requireNoUserErrors(productCreate.response, ['data', 'productCreate', 'userErrors'], 'productCreate setup');
  productId = requireString(readPath(productCreate.response, ['data', 'productCreate', 'product', 'id']), 'product id');

  createDefinition = await captureDocument('metafieldDefinitionCreate setup', createDefinitionDocumentPath, {
    definition: {
      name: 'Type From Definition',
      namespace,
      key,
      ownerType: 'PRODUCT',
      type: metafieldType,
      description: 'Temporary conformance definition for metafieldsSet omitted type capture',
    },
  });
  requireNoUserErrors(
    createDefinition.response,
    ['data', 'metafieldDefinitionCreate', 'userErrors'],
    'metafieldDefinitionCreate setup',
  );
  definitionId = requireString(
    readPath(createDefinition.response, ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id']),
    'definition id',
  );

  setWithoutType = await captureDocument('metafieldsSet without type', setWithoutTypeDocumentPath, {
    metafields: [
      {
        ownerId: productId,
        namespace,
        key,
        value: metafieldValue,
      },
    ],
  });
  requireNoUserErrors(setWithoutType.response, ['data', 'metafieldsSet', 'userErrors'], 'metafieldsSet without type');
  requirePathEquals(
    setWithoutType.response,
    ['data', 'metafieldsSet', 'metafields', '0', 'type'],
    metafieldType,
    'metafieldsSet returned type',
  );

  readAfterSet = await captureDocument('product read after metafieldsSet without type', readAfterSetDocumentPath, {
    id: productId,
    namespace,
    key,
  });
  requirePathEquals(
    readAfterSet.response,
    ['data', 'product', 'metafield', 'type'],
    metafieldType,
    'downstream product metafield type',
  );
  requirePathEquals(
    readAfterSet.response,
    ['data', 'product', 'metafield', 'value'],
    metafieldValue,
    'downstream product metafield value',
  );
} finally {
  if (definitionId) {
    cleanup.push(
      await captureQuery('cleanup metafieldDefinitionDelete', deleteDefinitionMutation, { id: definitionId }).catch(
        (error: unknown) => ({
          request: { query: deleteDefinitionMutation, variables: { id: definitionId } },
          status: 0,
          response: { error: String(error) },
        }),
      ),
    );
  }
  if (productId) {
    cleanup.push(
      await captureQuery('cleanup productDelete', productDeleteMutation, { input: { id: productId } }).catch(
        (error: unknown) => ({
          request: { query: productDeleteMutation, variables: { input: { id: productId } } },
          status: 0,
          response: { error: String(error) },
        }),
      ),
    );
  }
}

const fixture = {
  storeDomain,
  apiVersion,
  capturedAt: new Date().toISOString(),
  namespace,
  key,
  metafieldType,
  metafieldValue,
  productCreate: requireCapture(productCreate, 'productCreate'),
  createDefinition: requireCapture(createDefinition, 'createDefinition'),
  setWithoutType: requireCapture(setWithoutType, 'setWithoutType'),
  readAfterSet: requireCapture(readAfterSet, 'readAfterSet'),
  cleanup,
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({ ok: true, outputPath, productId, definitionId, namespace, key }, null, 2));
