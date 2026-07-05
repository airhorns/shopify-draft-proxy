/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type GraphqlResult = ConformanceGraphqlResult<JsonRecord>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const requestDir = path.join('config', 'parity-requests', 'discounts');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  accessToken: adminAccessToken,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

function readRunId(): string {
  const raw = process.env['SHOPIFY_CONFORMANCE_RUN_ID'];
  if (!raw) return String(Date.now());
  if (!/^[0-9]+$/u.test(raw)) {
    throw new Error(`SHOPIFY_CONFORMANCE_RUN_ID must be digits only, got ${JSON.stringify(raw)}`);
  }
  return raw;
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readPath(root: unknown, parts: string[]): unknown {
  let current = root;
  for (const part of parts) {
    if (!isRecord(current)) return undefined;
    current = current[part];
  }
  return current;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function requireStringPath(root: unknown, parts: string[], label: string): string {
  const value = readPath(root, parts);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing ${label}: ${JSON.stringify(root, null, 2)}`);
  }
  return value;
}

function userErrors(result: GraphqlResult, rootName: string): unknown[] {
  return readArray(readPath(result, ['payload', 'data', rootName, 'userErrors']));
}

function assertNoGraphqlErrors(result: GraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(result: GraphqlResult, rootName: string, label: string): void {
  assertNoGraphqlErrors(result, label);
  const errors = userErrors(result, rootName);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

async function runChecked(query: string, variables: JsonRecord, label: string): Promise<GraphqlResult> {
  const result = await runGraphqlRaw<JsonRecord>(query, variables);
  assertNoGraphqlErrors(result, label);
  return result;
}

function codeInput(title: string, code: string, startsAt: string): JsonRecord {
  return {
    title,
    code,
    startsAt,
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    customerGets: {
      value: {
        percentage: 0.1,
      },
      items: {
        all: true,
      },
    },
  };
}

function automaticInput(title: string, startsAt: string): JsonRecord {
  return {
    title,
    startsAt,
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    customerGets: {
      value: {
        percentage: 0.1,
      },
      items: {
        all: true,
      },
    },
  };
}

function assertQueryRead(result: GraphqlResult, expectedCodeTitle: string): void {
  const nodes = readArray(readPath(result, ['payload', 'data', 'discountNodes', 'nodes']));
  if (nodes.length !== 1) {
    throw new Error(`discountNodes(query:) expected one node, got ${JSON.stringify(nodes, null, 2)}`);
  }

  const node = nodes[0];
  const typename = requireStringPath(node, ['discount', '__typename'], 'discountNodes node typename');
  const title = requireStringPath(node, ['discount', 'title'], 'discountNodes node title');
  if (typename !== 'DiscountCodeBasic' || title !== expectedCodeTitle) {
    throw new Error(
      `discountNodes(query:) expected code discount ${expectedCodeTitle}, got ${JSON.stringify(node, null, 2)}`,
    );
  }

  const count = readPath(result, ['payload', 'data', 'discountNodesCount', 'count']);
  const precision = readPath(result, ['payload', 'data', 'discountNodesCount', 'precision']);
  if (count !== 1 || precision !== 'EXACT') {
    throw new Error(`discountNodesCount(query:) mismatch: ${JSON.stringify({ count, precision }, null, 2)}`);
  }
}

async function waitForRead(
  query: string,
  variables: JsonRecord,
  expectedCodeTitle: string,
  label: string,
): Promise<GraphqlResult> {
  let lastResult: GraphqlResult | null = null;
  let lastError: unknown = null;
  for (let attempt = 0; attempt < 40; attempt += 1) {
    lastResult = await runChecked(query, variables, label);
    try {
      assertQueryRead(lastResult, expectedCodeTitle);
      return lastResult;
    } catch (error) {
      lastError = error;
      await sleep(1500);
    }
  }
  throw new Error(`${label} did not converge: ${String(lastError)}; last=${JSON.stringify(lastResult, null, 2)}`);
}

const runId = readRunId();
const titleToken = `SDPQUERY${runId}`;
const codeTitle = titleToken;
const automaticTitle = titleToken;
const codeValue = `SDPQUERYCODE${runId}`;
const startsAt = new Date(Date.now() + 14 * 24 * 60 * 60 * 1000).toISOString();
const queryFilter = `title:${titleToken} method:code`;

const createDocument = await readRequest('discount-query-grammar-create.graphql');
const readDocument = await readRequest('discount-query-grammar-read.graphql');

const deleteCodeDocument = `#graphql
  mutation DiscountQueryGrammarDeleteCode($id: ID!) {
    discountCodeDelete(id: $id) {
      deletedCodeDiscountId
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
  }
`;

const deleteAutomaticDocument = `#graphql
  mutation DiscountQueryGrammarDeleteAutomatic($id: ID!) {
    discountAutomaticDelete(id: $id) {
      deletedAutomaticDiscountId
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
  }
`;

const createVariables: JsonRecord = {
  code: codeInput(codeTitle, codeValue, startsAt),
  automatic: automaticInput(automaticTitle, startsAt),
};

const createdCodeIds: string[] = [];
const createdAutomaticIds: string[] = [];
const cleanupResults: GraphqlResult[] = [];

async function cleanupCreatedDiscounts(): Promise<void> {
  for (const id of createdCodeIds) {
    cleanupResults.push(await runGraphqlRaw<JsonRecord>(deleteCodeDocument, { id }));
  }
  for (const id of createdAutomaticIds) {
    cleanupResults.push(await runGraphqlRaw<JsonRecord>(deleteAutomaticDocument, { id }));
  }
}

let create: GraphqlResult | null = null;
let read: GraphqlResult | null = null;

try {
  create = await runChecked(createDocument, createVariables, 'discount query grammar create');
  assertNoUserErrors(create, 'code', 'discount query grammar code create');
  assertNoUserErrors(create, 'automatic', 'discount query grammar automatic create');

  const codeId = requireStringPath(create, ['payload', 'data', 'code', 'codeDiscountNode', 'id'], 'code discount id');
  const automaticId = requireStringPath(
    create,
    ['payload', 'data', 'automatic', 'automaticDiscountNode', 'id'],
    'automatic discount id',
  );
  createdCodeIds.push(codeId);
  createdAutomaticIds.push(automaticId);

  const readVariables: JsonRecord = {
    query: queryFilter,
    first: 10,
  };
  read = await waitForRead(readDocument, readVariables, codeTitle, 'discount query grammar read');

  await cleanupCreatedDiscounts();

  const output = {
    variables: {
      runId,
      titleToken,
      query: queryFilter,
      first: 10,
      startsAt,
      codeTitle,
      automaticTitle,
      codeValue,
      codeId,
      automaticId,
    },
    requests: {
      create: { query: createDocument, variables: createVariables },
      read: { query: readDocument, variables: readVariables },
      cleanupCode: { query: deleteCodeDocument },
      cleanupAutomatic: { query: deleteAutomaticDocument },
    },
    scopeProbe,
    create,
    read,
    cleanup: cleanupResults,
  };

  const outputPath = path.join(outputDir, 'discount-query-grammar.json');
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        codeId,
        automaticId,
      },
      null,
      2,
    ),
  );
} catch (error) {
  if (createdCodeIds.length > 0 || createdAutomaticIds.length > 0) {
    await cleanupCreatedDiscounts();
    console.error(`Cleaned up created discounts after capture failure: ${JSON.stringify(cleanupResults, null, 2)}`);
  }
  throw error;
}
