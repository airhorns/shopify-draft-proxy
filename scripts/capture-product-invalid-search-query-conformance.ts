/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureEntry = {
  query: string;
  variables: Record<string, unknown>;
  result: {
    status: number;
    payload: unknown;
  };
};

const searchIndexWaitMs = 12_000;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-invalid-search-query-syntax.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readRequest(name: string): Promise<string> {
  return readFile(path.join('config', 'parity-requests', 'products', name), 'utf8');
}

async function capture(query: string, variables: Record<string, unknown>): Promise<CaptureEntry> {
  const result: ConformanceGraphqlResult = await runGraphqlRequest(query, variables);
  return {
    query,
    variables,
    result: {
      status: result.status,
      payload: result.payload,
    },
  };
}

function readRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    current = readRecord(current)?.[part];
  }
  return current;
}

function assertNoTopLevelErrors(entry: CaptureEntry, context: string): void {
  const payload = readRecord(entry.result.payload);
  if (entry.result.status < 200 || entry.result.status >= 300 || payload?.['errors'] !== undefined) {
    throw new Error(`${context} returned top-level errors: ${JSON.stringify(entry.result.payload)}`);
  }
}

function readUserErrors(entry: CaptureEntry, pathParts: string[]): unknown[] {
  const errors = readPath(entry.result.payload, pathParts);
  return Array.isArray(errors) ? errors : [];
}

function readProductId(entry: CaptureEntry): string {
  const id = readPath(entry.result.payload, ['data', 'productCreate', 'product', 'id']);
  if (typeof id !== 'string') {
    throw new Error(`Expected productCreate to return a product id: ${JSON.stringify(entry.result.payload)}`);
  }
  return id;
}

function readCount(entry: CaptureEntry): number {
  const count = readPath(entry.result.payload, ['data', 'productsCount', 'count']);
  if (typeof count !== 'number') {
    throw new Error(`Expected productsCount.count in search capture: ${JSON.stringify(entry.result.payload)}`);
  }
  return count;
}

const createDocument = await readRequest('product-invalid-search-query-create.graphql');
const searchDocument = await readRequest('product-invalid-search-query-search.graphql');

const runId = `har-549-${Date.now()}`;
const productTitle = `${runId} Invalid Search Query`;
const productTag = runId;

const productCreateVariables = {
  input: {
    title: productTitle,
    tags: [productTag],
    status: 'ACTIVE',
  },
};

const productCreate = await capture(createDocument, productCreateVariables);
assertNoTopLevelErrors(productCreate, 'product invalid-search productCreate');
if (readUserErrors(productCreate, ['data', 'productCreate', 'userErrors']).length > 0) {
  throw new Error(`Expected productCreate userErrors to be empty: ${JSON.stringify(productCreate.result.payload)}`);
}

const productId = readProductId(productCreate);

const deleteDocument = `#graphql
  mutation ProductInvalidSearchQueryCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

let cleanupDelete: CaptureEntry | null = null;

try {
  await sleep(searchIndexWaitMs);

  const validTagSearchAfterCreate = await capture(searchDocument, {
    query: `tag:${productTag}`,
  });
  assertNoTopLevelErrors(validTagSearchAfterCreate, 'product valid tag search after create');
  if (readCount(validTagSearchAfterCreate) !== 1) {
    throw new Error(
      `Expected valid tag search to find the disposable product: ${JSON.stringify(validTagSearchAfterCreate.result.payload)}`,
    );
  }

  const fieldOpenParenSearchAfterCreate = await capture(searchDocument, {
    query: `tag:(${productTag}`,
  });
  assertNoTopLevelErrors(fieldOpenParenSearchAfterCreate, 'product field open-paren search after create');
  if (readCount(fieldOpenParenSearchAfterCreate) !== 0) {
    throw new Error(
      `Expected field open-paren query to return zero matches: ${JSON.stringify(fieldOpenParenSearchAfterCreate.result.payload)}`,
    );
  }

  const fieldQuotedOpenParenSearchAfterCreate = await capture(searchDocument, {
    query: `tag:("${productTag}"`,
  });
  assertNoTopLevelErrors(fieldQuotedOpenParenSearchAfterCreate, 'product field quoted open-paren search after create');
  if (readCount(fieldQuotedOpenParenSearchAfterCreate) !== 0) {
    throw new Error(
      `Expected field quoted open-paren query to return zero matches: ${JSON.stringify(fieldQuotedOpenParenSearchAfterCreate.result.payload)}`,
    );
  }

  const bareLeadingParenSearchAfterCreate = await capture(searchDocument, {
    query: `(${productTag}`,
  });
  assertNoTopLevelErrors(bareLeadingParenSearchAfterCreate, 'product bare leading-paren search after create');
  if (readCount(bareLeadingParenSearchAfterCreate) !== 1) {
    throw new Error(
      `Expected bare leading-paren query to keep matching the disposable product: ${JSON.stringify(bareLeadingParenSearchAfterCreate.result.payload)}`,
    );
  }

  const danglingOrSearchAfterCreate = await capture(searchDocument, {
    query: `tag:${productTag} OR`,
  });
  assertNoTopLevelErrors(danglingOrSearchAfterCreate, 'product dangling OR search after create');
  if (readCount(danglingOrSearchAfterCreate) !== 1) {
    throw new Error(
      `Expected dangling OR query to keep matching the disposable product: ${JSON.stringify(danglingOrSearchAfterCreate.result.payload)}`,
    );
  }

  cleanupDelete = await capture(deleteDocument, { input: { id: productId } });
  assertNoTopLevelErrors(cleanupDelete, 'product invalid-search cleanup delete');

  const captureFile = {
    notes: [
      'HAR-549 capture for malformed Shopify Admin product search query syntax.',
      'Shopify returned normal data envelopes with no top-level errors for these malformed-looking queries.',
      '`tag:(value` and `tag:("value"` behaved as literal non-matching field values; bare leading `(` and dangling `OR` were forgiving and still matched the disposable product.',
      `The capture waited ${searchIndexWaitMs}ms after productCreate so product tag search was visible before probing malformed queries.`,
    ],
    run: {
      runId,
      productTitle,
      productTag,
      searchIndexWaitMs,
    },
    captures: {
      productCreate,
      validTagSearchAfterCreate,
      fieldOpenParenSearchAfterCreate,
      fieldQuotedOpenParenSearchAfterCreate,
      bareLeadingParenSearchAfterCreate,
      danglingOrSearchAfterCreate,
      cleanupDelete,
    },
    upstreamCalls: [],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(captureFile, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
} finally {
  if (cleanupDelete === null) {
    const cleanup = await capture(deleteDocument, { input: { id: productId } });
    console.log(`Cleanup result: ${JSON.stringify(cleanup.result.payload)}`);
  }
}
