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

function titlesFromNodes(result: GraphqlResult, connectionName: string, titlePath: string[]): string[] {
  return readArray(readPath(result, ['payload', 'data', connectionName, 'nodes'])).map((node) =>
    requireStringPath(node, titlePath, `${connectionName} node title`),
  );
}

function nestedCodes(result: GraphqlResult): string[] {
  return readArray(readPath(result, ['payload', 'data', 'nestedCode', 'codeDiscount', 'codes', 'nodes'])).map((node) =>
    requireStringPath(node, ['code'], 'nested code'),
  );
}

function assertArrayEquals(label: string, actual: string[], expected: string[]): void {
  if (actual.length !== expected.length || actual.some((value, index) => value !== expected[index])) {
    throw new Error(`${label}: expected ${JSON.stringify(expected)} got ${JSON.stringify(actual)}`);
  }
}

function assertConnectionBooleans(
  result: GraphqlResult,
  connectionName: string,
  expected: { hasNextPage: boolean; hasPreviousPage: boolean },
): void {
  const pageInfo = readPath(result, ['payload', 'data', connectionName, 'pageInfo']);
  if (
    readPath(pageInfo, ['hasNextPage']) !== expected.hasNextPage ||
    readPath(pageInfo, ['hasPreviousPage']) !== expected.hasPreviousPage
  ) {
    throw new Error(`${connectionName} pageInfo mismatch: ${JSON.stringify(pageInfo, null, 2)}`);
  }
}

function assertNestedCodeWindow(result: GraphqlResult, expectedCodes: string[]): void {
  assertArrayEquals('nested codes(first: 2)', nestedCodes(result), expectedCodes);
  const pageInfo = readPath(result, ['payload', 'data', 'nestedCode', 'codeDiscount', 'codes', 'pageInfo']);
  if (readPath(pageInfo, ['hasNextPage']) !== true || readPath(pageInfo, ['hasPreviousPage']) !== false) {
    throw new Error(`nested codes pageInfo mismatch: ${JSON.stringify(pageInfo, null, 2)}`);
  }
}

function assertReadFirst(result: GraphqlResult, expected: ExpectedTitles): void {
  assertArrayEquals(
    'discountNodes reverse first page',
    titlesFromNodes(result, 'discountNodes', ['discount', 'title']),
    [expected.zuluCode, expected.yankeeAutomatic],
  );
  assertConnectionBooleans(result, 'discountNodes', { hasNextPage: true, hasPreviousPage: false });
  assertArrayEquals(
    'codeDiscountNodes reverse window',
    titlesFromNodes(result, 'codeDiscountNodesReverse', ['codeDiscount', 'title']),
    [expected.zuluCode, expected.bravoCode],
  );
  assertArrayEquals(
    'automaticDiscountNodes reverse window',
    titlesFromNodes(result, 'automaticDiscountNodesReverse', ['automaticDiscount', 'title']),
    [expected.alphaAutomatic, expected.yankeeAutomatic],
  );
  const limitedCount = readPath(result, ['payload', 'data', 'limitedCount']);
  if (readPath(limitedCount, ['count']) !== 2 || readPath(limitedCount, ['precision']) !== 'AT_LEAST') {
    throw new Error(`discountNodesCount(limit:) mismatch: ${JSON.stringify(limitedCount, null, 2)}`);
  }
  assertNestedCodeWindow(result, [expected.zuluSeedCode, expected.zuluAddedCode]);
}

function assertReadAfter(result: GraphqlResult, expected: ExpectedTitles): void {
  assertArrayEquals(
    'discountNodes reverse after page',
    titlesFromNodes(result, 'discountNodes', ['discount', 'title']),
    [expected.bravoCode, expected.alphaAutomatic],
  );
  assertConnectionBooleans(result, 'discountNodes', { hasNextPage: false, hasPreviousPage: true });
}

async function waitForRead(
  query: string,
  variables: JsonRecord,
  expected: ExpectedTitles,
  label: string,
): Promise<GraphqlResult> {
  let lastResult: GraphqlResult | null = null;
  let lastError: unknown = null;
  for (let attempt = 0; attempt < 16; attempt += 1) {
    lastResult = await runChecked(query, variables, label);
    try {
      assertReadFirst(lastResult, expected);
      return lastResult;
    } catch (error) {
      lastError = error;
      await sleep(750);
    }
  }
  throw new Error(`${label} did not converge: ${String(lastError)}; last=${JSON.stringify(lastResult, null, 2)}`);
}

type ExpectedTitles = {
  alphaAutomatic: string;
  bravoCode: string;
  yankeeAutomatic: string;
  zuluCode: string;
  zuluSeedCode: string;
  zuluAddedCode: string;
};

const runId = readRunId();
const titlePrefix = `zzzzzz SDP connection ${runId}`;
const titles: ExpectedTitles = {
  alphaAutomatic: `${titlePrefix} Alpha automatic`,
  bravoCode: `${titlePrefix} Bravo code`,
  yankeeAutomatic: `${titlePrefix} Yankee automatic`,
  zuluCode: `${titlePrefix} Zulu code`,
  zuluSeedCode: `SDPCONNZSEED${runId}`,
  zuluAddedCode: `SDPCONNZADD${runId}`,
};
const zuluSecondAddedCode = `SDPCONNZPLUS${runId}`;
const zuluThirdAddedCode = `SDPCONNZTAIL${runId}`;
const bravoSeedCode = `SDPCONNBSEED${runId}`;
const startsAt = new Date(Date.now() + 14 * 24 * 60 * 60 * 1000).toISOString();
const queryFilter = 'status:scheduled';

const createDocument = await readRequest('discount-connection-mechanics-create.graphql');
const bulkAddDocument = await readRequest('discount-connection-mechanics-bulk-add.graphql');
const readFirstDocument = await readRequest('discount-connection-mechanics-read-first.graphql');
const readAfterDocument = await readRequest('discount-connection-mechanics-read-after.graphql');

const deleteCodeDocument = `#graphql
  mutation DiscountConnectionMechanicsDeleteCode($id: ID!) {
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
  mutation DiscountConnectionMechanicsDeleteAutomatic($id: ID!) {
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
  zuluCode: codeInput(titles.zuluCode, titles.zuluSeedCode, startsAt),
  yankeeAutomatic: automaticInput(titles.yankeeAutomatic, startsAt),
  bravoCode: codeInput(titles.bravoCode, bravoSeedCode, startsAt),
  alphaAutomatic: automaticInput(titles.alphaAutomatic, startsAt),
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
let addCodes: GraphqlResult | null = null;
let readFirst: GraphqlResult | null = null;
let readAfter: GraphqlResult | null = null;

try {
  create = await runChecked(createDocument, createVariables, 'discount connection mechanics create');
  for (const rootName of ['zuluCode', 'yankeeAutomatic', 'bravoCode', 'alphaAutomatic']) {
    assertNoUserErrors(create, rootName, `discount connection mechanics ${rootName}`);
  }

  const zuluCodeId = requireStringPath(
    create,
    ['payload', 'data', 'zuluCode', 'codeDiscountNode', 'id'],
    'zulu code discount id',
  );
  const bravoCodeId = requireStringPath(
    create,
    ['payload', 'data', 'bravoCode', 'codeDiscountNode', 'id'],
    'bravo code discount id',
  );
  const yankeeAutomaticId = requireStringPath(
    create,
    ['payload', 'data', 'yankeeAutomatic', 'automaticDiscountNode', 'id'],
    'yankee automatic discount id',
  );
  const alphaAutomaticId = requireStringPath(
    create,
    ['payload', 'data', 'alphaAutomatic', 'automaticDiscountNode', 'id'],
    'alpha automatic discount id',
  );

  createdCodeIds.push(zuluCodeId, bravoCodeId);
  createdAutomaticIds.push(yankeeAutomaticId, alphaAutomaticId);

  const addVariables: JsonRecord = {
    discountId: zuluCodeId,
    codes: [{ code: titles.zuluAddedCode }, { code: zuluSecondAddedCode }, { code: zuluThirdAddedCode }],
  };
  addCodes = await runChecked(bulkAddDocument, addVariables, 'discount connection mechanics bulk add');
  assertNoUserErrors(addCodes, 'discountRedeemCodeBulkAdd', 'discount connection mechanics bulk add');

  const readFirstVariables: JsonRecord = {
    query: queryFilter,
    first: 2,
    nestedId: zuluCodeId,
    countLimit: 2,
  };
  readFirst = await waitForRead(readFirstDocument, readFirstVariables, titles, 'discount connection mechanics read');

  const endCursor = requireStringPath(
    readFirst,
    ['payload', 'data', 'discountNodes', 'pageInfo', 'endCursor'],
    'discountNodes first page endCursor',
  );
  const readAfterVariables: JsonRecord = {
    query: queryFilter,
    first: 2,
    after: endCursor,
  };
  readAfter = await runChecked(readAfterDocument, readAfterVariables, 'discount connection mechanics read after');
  assertReadAfter(readAfter, titles);

  await cleanupCreatedDiscounts();

  const output = {
    variables: {
      runId,
      query: queryFilter,
      first: 2,
      countLimit: 2,
      startsAt,
      ...titles,
      zuluSecondAddedCode,
      zuluThirdAddedCode,
      bravoSeedCode,
      zuluCodeId,
      bravoCodeId,
      yankeeAutomaticId,
      alphaAutomaticId,
    },
    requests: {
      create: { query: createDocument, variables: createVariables },
      addCodes: { query: bulkAddDocument, variables: addVariables },
      readFirst: { query: readFirstDocument, variables: readFirstVariables },
      readAfter: { query: readAfterDocument, variables: readAfterVariables },
      cleanupCode: { query: deleteCodeDocument },
      cleanupAutomatic: { query: deleteAutomaticDocument },
    },
    scopeProbe,
    create,
    addCodes,
    readFirst,
    readAfter,
    cleanup: cleanupResults,
  };

  const outputPath = path.join(outputDir, 'discount-connection-mechanics.json');
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        zuluCodeId,
        bravoCodeId,
        yankeeAutomaticId,
        alphaAutomaticId,
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
