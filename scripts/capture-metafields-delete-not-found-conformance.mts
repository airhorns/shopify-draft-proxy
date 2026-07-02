/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type Capture = {
  name: string;
  request: {
    query: string;
    variables: JsonRecord;
  };
  status: number;
  response: unknown;
};

const requestPaths = {
  setup: 'config/parity-requests/metafield-definitions/metafield-delete-not-found-setup.graphql',
  delete: 'config/parity-requests/metafield-definitions/metafields-delete-not-found.graphql',
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafield-definitions');
const outputPath = path.join(outputDir, 'metafield-delete-not-found.json');
const runId = Date.now().toString(36);
const namespace = 'custom';
const key = `delete_not_found_${runId}`;
const neverCreatedKey = `never_created_${runId}`;

const productCreateMutation = `#graphql
  mutation MetafieldDeleteNotFoundProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product { id title }
      userErrors { field message }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation MetafieldDeleteNotFoundProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

function readObject(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    const object = readObject(current);
    if (object === null) {
      return undefined;
    }
    current = object[part];
  }

  return current;
}

function readStringPath(value: unknown, pathParts: string[], label: string): string {
  const pathValue = readPath(value, pathParts);
  if (typeof pathValue !== 'string' || pathValue.length === 0) {
    throw new Error(`${label} did not return a string at ${pathParts.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }

  return pathValue;
}

function readUserErrors(payload: unknown, pathParts: string[]): unknown[] {
  const userErrors = readPath(payload, pathParts);
  return Array.isArray(userErrors) ? userErrors : [];
}

function readDeletedMetafields(payload: unknown, label: string): unknown[] {
  const deletedMetafields = readPath(payload, ['data', 'metafieldsDelete', 'deletedMetafields']);
  if (!Array.isArray(deletedMetafields)) {
    throw new Error(`${label} did not return deletedMetafields: ${JSON.stringify(payload, null, 2)}`);
  }

  return deletedMetafields;
}

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} failed HTTP status: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoGraphqlErrors(result: ConformanceGraphqlResult, label: string): void {
  assertHttpOk(result, label);
  const errors = readPath(result.payload, ['errors']);
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`${label} returned GraphQL errors: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertDeletedMetafieldIdentifier(
  payload: unknown,
  expected: { key: string; namespace: string; ownerId: string },
  label: string,
): void {
  const firstDeleted = readObject(readDeletedMetafields(payload, label)[0]);
  if (
    firstDeleted?.['ownerId'] !== expected.ownerId ||
    firstDeleted?.['namespace'] !== expected.namespace ||
    firstDeleted?.['key'] !== expected.key
  ) {
    throw new Error(`${label} did not return the deleted identifier: ${JSON.stringify(payload, null, 2)}`);
  }
}

function assertDeletedMetafieldNull(payload: unknown, label: string): void {
  if (readDeletedMetafields(payload, label)[0] !== null) {
    throw new Error(`${label} did not return an ordered null deletedMetafields entry: ${JSON.stringify(payload)}`);
  }
}

function captureFromResult(
  name: string,
  query: string,
  variables: JsonRecord,
  result: ConformanceGraphqlResult,
): Capture {
  return {
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function captureGraphql(name: string, query: string, variables: JsonRecord): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertNoGraphqlErrors(result, name);
  return captureFromResult(name, query, variables, result);
}

let productId: string | undefined;
const cleanup: Capture[] = [];

async function cleanupProduct(): Promise<void> {
  if (!productId) {
    return;
  }

  const productDelete = await captureGraphql('cleanup-product-delete', productDeleteMutation, {
    input: { id: productId },
  });
  cleanup.push(productDelete);
  assertNoUserErrors(productDelete.response, ['data', 'productDelete', 'userErrors'], 'cleanup productDelete');
  productId = undefined;
}

try {
  const productCreate = await captureGraphql('product-create-setup', productCreateMutation, {
    product: {
      title: `Metafields delete not found ${runId}`,
      status: 'DRAFT',
    },
  });
  assertNoUserErrors(productCreate.response, ['data', 'productCreate', 'userErrors'], 'productCreate setup');
  productId = readStringPath(productCreate.response, ['data', 'productCreate', 'product', 'id'], 'productCreate setup');

  const identifier = {
    ownerId: productId,
    namespace,
    key,
  };
  const neverCreatedIdentifier = {
    ownerId: productId,
    namespace,
    key: neverCreatedKey,
  };

  const setup = await captureGraphql('setup-metafields-set', queries.setup, {
    metafields: [
      {
        ...identifier,
        type: 'single_line_text_field',
        value: `delete me ${runId}`,
      },
    ],
  });
  assertNoUserErrors(setup.response, ['data', 'metafieldsSet', 'userErrors'], 'metafieldsSet setup');

  const deleteExisting = await captureGraphql('delete-existing-identifier', queries.delete, {
    metafields: [identifier],
  });
  assertNoUserErrors(deleteExisting.response, ['data', 'metafieldsDelete', 'userErrors'], 'delete existing');
  assertDeletedMetafieldIdentifier(deleteExisting.response, identifier, 'delete existing');

  const repeatDelete = await captureGraphql('repeat-delete-by-just-deleted-identifier', queries.delete, {
    metafields: [identifier],
  });
  assertNoUserErrors(repeatDelete.response, ['data', 'metafieldsDelete', 'userErrors'], 'repeat delete');
  assertDeletedMetafieldNull(repeatDelete.response, 'repeat delete');

  const neverCreatedDelete = await captureGraphql('delete-never-created-identifier', queries.delete, {
    metafields: [neverCreatedIdentifier],
  });
  assertNoUserErrors(neverCreatedDelete.response, ['data', 'metafieldsDelete', 'userErrors'], 'never-created delete');
  assertDeletedMetafieldNull(neverCreatedDelete.response, 'never-created delete');

  await cleanupProduct();

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        summary:
          'metafieldsSet plus metafieldsDelete not-found behavior for product-owned metafields: existing identifier deletes, repeat deletes return ordered nulls, and never-created identifiers return ordered nulls.',
        seed: {
          runId,
          productId: identifier.ownerId,
          namespace,
          key,
          neverCreatedKey,
        },
        productCreate,
        setup,
        deleteExisting,
        repeatDelete,
        neverCreatedDelete,
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
  console.log(JSON.stringify({ ok: true, outputPath, productId: identifier.ownerId, runId }, null, 2));
} catch (error) {
  try {
    await cleanupProduct();
  } catch (cleanupError) {
    console.error(
      `Cleanup failed after capture error: ${cleanupError instanceof Error ? cleanupError.message : cleanupError}`,
    );
  }
  throw error;
}
