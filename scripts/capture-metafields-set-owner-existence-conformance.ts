/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonObject = Record<string, unknown>;

type CapturedRequest = {
  query: string;
  variables: JsonObject;
  status: number;
  response: ConformanceGraphqlPayload;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const setDocumentPath = 'config/parity-requests/products/metafieldsSet-parity-plan.graphql';
const existenceDocumentPath = 'config/parity-requests/products/metafieldsSet-owner-existence-hydrate.graphql';
const readDocumentPath = 'config/parity-requests/products/metafieldsSet-owner-existence-read.graphql';
const [setDocument, existenceDocument, readDocument] = await Promise.all([
  readFile(setDocumentPath, 'utf8'),
  readFile(existenceDocumentPath, 'utf8'),
  readFile(readDocumentPath, 'utf8'),
]);

const productCreateDocument = `#graphql
  mutation MetafieldsOwnerExistenceCreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
        totalInventory
        tracksInventory
        createdAt
        updatedAt
      }
      userErrors { field message }
    }
  }
`;

const productDeleteDocument = `#graphql
  mutation MetafieldsOwnerExistenceDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

function isObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function getPath(value: unknown, parts: string[]): unknown {
  let cursor = value;
  for (const part of parts) {
    if (!isObject(cursor)) return undefined;
    cursor = cursor[part];
  }
  return cursor;
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return a string: ${JSON.stringify(value)}`);
  }
  return value;
}

function requireEqual(actual: unknown, expected: unknown, label: string): void {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${label} mismatch: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

function requireSuccess(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result.payload)}`);
  }
}

async function capture(query: string, variables: JsonObject, label: string): Promise<CapturedRequest> {
  const result = await runGraphqlRaw(query, variables);
  requireSuccess(result, label);
  return { query, variables, status: result.status, response: result.payload };
}

function upstreamCall(capture: CapturedRequest, operationName: string): JsonObject {
  return {
    method: 'POST',
    apiSurface: 'admin',
    apiVersion,
    path: `/admin/api/${apiVersion}/graphql.json`,
    operationName,
    variables: capture.variables,
    query: capture.query,
    response: { status: capture.status, body: capture.response },
  };
}

const runId = `${Date.now()}`;
const namespace = `owner_existence_${runId}`;
const key = 'atomicity';
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'metafields-set-owner-existence-atomicity.json');
let validOwnerId: string | null = null;
let deletedOwnerId: string | null = null;
let validOwnerCleaned = false;
let deletedOwnerCleaned = false;

try {
  const validSetup = await capture(
    productCreateDocument,
    { product: { title: `Metafields owner existence valid ${runId}`, status: 'DRAFT' } },
    'valid productCreate',
  );
  requireEqual(getPath(validSetup.response, ['data', 'productCreate', 'userErrors']), [], 'valid productCreate');
  validOwnerId = requireString(
    getPath(validSetup.response, ['data', 'productCreate', 'product', 'id']),
    'valid productCreate id',
  );

  const deletedSetup = await capture(
    productCreateDocument,
    { product: { title: `Metafields owner existence deleted ${runId}`, status: 'DRAFT' } },
    'deleted productCreate',
  );
  requireEqual(getPath(deletedSetup.response, ['data', 'productCreate', 'userErrors']), [], 'deleted productCreate');
  deletedOwnerId = requireString(
    getPath(deletedSetup.response, ['data', 'productCreate', 'product', 'id']),
    'deleted productCreate id',
  );

  const deletedSetupCleanup = await capture(
    productDeleteDocument,
    { input: { id: deletedOwnerId } },
    'deleted owner productDelete',
  );
  requireEqual(
    getPath(deletedSetupCleanup.response, ['data', 'productDelete', 'userErrors']),
    [],
    'deleted owner productDelete',
  );
  deletedOwnerCleaned = true;

  const singleExistence = await capture(existenceDocument, { ids: [deletedOwnerId] }, 'single owner existence hydrate');
  requireEqual(getPath(singleExistence.response, ['data', 'nodes']), [null], 'single deleted owner existence');

  const singleVariables = {
    metafields: [
      {
        ownerId: deletedOwnerId,
        namespace,
        key,
        type: 'single_line_text_field',
        value: 'must not persist',
      },
    ],
  };
  const singleInvalid = await capture(setDocument, singleVariables, 'single invalid metafieldsSet');
  requireEqual(
    getPath(singleInvalid.response, ['data', 'metafieldsSet']),
    {
      metafields: [],
      userErrors: [
        {
          field: ['metafields', '0', 'ownerId'],
          message: 'Owner does not exist.',
          code: 'INVALID_VALUE',
          elementIndex: null,
        },
      ],
    },
    'single invalid metafieldsSet payload',
  );

  const mixedIds = [validOwnerId, deletedOwnerId].sort();
  const mixedExistence = await capture(existenceDocument, { ids: mixedIds }, 'mixed owner existence hydrate');
  const mixedExistenceNodes = getPath(mixedExistence.response, ['data', 'nodes']);
  if (!Array.isArray(mixedExistenceNodes)) {
    throw new Error(`mixed owner existence did not return nodes: ${JSON.stringify(mixedExistence.response)}`);
  }
  requireEqual(
    mixedExistenceNodes.map((node) => (isObject(node) ? node['id'] : null)),
    mixedIds.map((id) => (id === validOwnerId ? validOwnerId : null)),
    'mixed owner existence ordering',
  );

  const mixedVariables = {
    metafields: [
      {
        ownerId: validOwnerId,
        namespace,
        key,
        type: 'single_line_text_field',
        value: 'valid row must roll back',
      },
      {
        ownerId: deletedOwnerId,
        namespace,
        key,
        type: 'single_line_text_field',
        value: 'missing row must fail',
      },
    ],
  };
  const mixedInvalid = await capture(setDocument, mixedVariables, 'mixed invalid metafieldsSet');
  requireEqual(
    getPath(mixedInvalid.response, ['data', 'metafieldsSet']),
    {
      metafields: [],
      userErrors: [
        {
          field: ['metafields', '1', 'ownerId'],
          message: 'Owner does not exist.',
          code: 'INVALID_VALUE',
          elementIndex: null,
        },
      ],
    },
    'mixed invalid metafieldsSet payload',
  );

  const downstreamRead = await capture(
    readDocument,
    { id: validOwnerId, namespace, key },
    'mixed batch downstream read',
  );
  requireEqual(
    getPath(downstreamRead.response, ['data', 'product', 'metafield']),
    null,
    'mixed batch singular metafield read',
  );
  requireEqual(
    getPath(downstreamRead.response, ['data', 'product', 'metafields', 'nodes']),
    [],
    'mixed batch metafields connection read',
  );

  const validSetupCleanup = await capture(
    productDeleteDocument,
    { input: { id: validOwnerId } },
    'valid owner cleanup',
  );
  validOwnerCleaned = true;

  const fixture = {
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    setup: { validOwner: validSetup, deletedOwner: deletedSetup, deletedOwnerCleanup: deletedSetupCleanup },
    singleInvalid,
    mixedInvalid,
    downstreamRead,
    cleanup: { validOwner: validSetupCleanup },
    upstreamCalls: [
      upstreamCall(singleExistence, 'OwnerMetafieldsExistenceHydrate'),
      upstreamCall(mixedExistence, 'OwnerMetafieldsExistenceHydrate'),
      upstreamCall(downstreamRead, 'MetafieldsSetOwnerExistenceRead'),
    ],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  const formatResult = spawnSync('corepack', ['pnpm', 'exec', 'oxfmt', outputPath], { stdio: 'inherit' });
  if (formatResult.status !== 0) {
    throw new Error(`Failed to format captured fixture: ${outputPath}`);
  }
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (validOwnerId && !validOwnerCleaned) {
    await runGraphqlRaw(productDeleteDocument, { input: { id: validOwnerId } });
  }
  if (deletedOwnerId && !deletedOwnerCleaned) {
    await runGraphqlRaw(productDeleteDocument, { input: { id: deletedOwnerId } });
  }
}
