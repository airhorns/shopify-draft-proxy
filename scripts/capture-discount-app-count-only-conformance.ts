/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
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

type CaptureCall = {
  operationName: string;
  variables: JsonRecord;
  query: string;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-app-count-only-read-after-create.json');
const requestDir = path.join('config', 'parity-requests', 'discounts');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  accessToken: adminAccessToken,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const functionCatalogDocument = `#graphql
  query DiscountAppCountOnlyFunctionCatalog {
    shopifyFunctions(first: 50) {
      nodes {
        id
        title
        apiType
        description
        appKey
        app {
          id
          title
          handle
          apiKey
        }
      }
    }
  }
`;

const functionHydrateByIdDocument = `query ShopifyFunctionById($id: String!) {
  shopifyFunction(id: $id) {
    id
    title
    apiType
    description
    appKey
    app {
      id
      title
      handle
      apiKey
    }
  }
}
`;

const cleanupAutomaticDocument = `#graphql
  mutation DiscountAppCountOnlyCleanupAutomatic($id: ID!) {
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

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let cursor = value;
  for (const part of pathParts) {
    if (cursor === null || typeof cursor !== 'object') return undefined;
    cursor = (cursor as Record<string, unknown>)[part];
  }
  return cursor;
}

function requireStringPath(value: unknown, pathParts: string[], context: string): string {
  const candidate = readPath(value, pathParts);
  if (typeof candidate !== 'string' || candidate.length === 0) {
    throw new Error(`${context} missing string at ${pathParts.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return candidate;
}

function readCount(result: GraphqlResult, key: string): number {
  const count = readPath(result, ['payload', 'data', key, 'count']);
  if (typeof count !== 'number' || !Number.isInteger(count) || count < 0) {
    throw new Error(`${key}.count missing integer: ${JSON.stringify(result, null, 2)}`);
  }
  return count;
}

function readPrecision(result: GraphqlResult, key: string): string {
  const precision = readPath(result, ['payload', 'data', key, 'precision']);
  if (precision !== 'EXACT' && precision !== 'AT_LEAST') {
    throw new Error(`${key}.precision missing CountPrecision: ${JSON.stringify(result, null, 2)}`);
  }
  return precision;
}

function readFunctionNodes(catalog: GraphqlResult): JsonRecord[] {
  return readArray(readPath(catalog, ['payload', 'data', 'shopifyFunctions', 'nodes'])).flatMap((node) => {
    const record = readRecord(node);
    return record ? [record] : [];
  });
}

function findDiscountFunction(nodes: JsonRecord[]): JsonRecord {
  const node = nodes.find((candidate) => readString(candidate['apiType'])?.toLowerCase() === 'discount');
  if (!node) {
    throw new Error(`Expected a released discount Shopify Function in the conformance app: ${JSON.stringify(nodes)}`);
  }
  return node;
}

function assertNoGraphqlErrors(result: GraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || readRecord(result.payload)?.['errors']) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(result: GraphqlResult, rootName: string, context: string): void {
  assertNoGraphqlErrors(result, context);
  const userErrors = readArray(readPath(result, ['payload', 'data', rootName, 'userErrors']));
  if (userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

async function runChecked(query: string, variables: JsonRecord, context: string): Promise<GraphqlResult> {
  const result = await runGraphqlRaw<JsonRecord>(query, variables);
  assertNoGraphqlErrors(result, context);
  return result;
}

async function waitForCount(
  query: string,
  variables: JsonRecord,
  expectedCount: number,
  expectedLimitedCount: number,
  expectedLimitedPrecision: 'EXACT' | 'AT_LEAST',
  context: string,
): Promise<GraphqlResult> {
  let latest: GraphqlResult | null = null;
  for (let attempt = 1; attempt <= 20; attempt += 1) {
    latest = await runChecked(query, variables, `${context} attempt ${attempt}`);
    if (
      readCount(latest, 'appCount') === expectedCount &&
      readCount(latest, 'limitedAppCount') === expectedLimitedCount &&
      readPrecision(latest, 'limitedAppCount') === expectedLimitedPrecision
    ) {
      return latest;
    }
    await sleep(1_000);
  }
  throw new Error(`${context} did not reach expected count ${expectedCount}: ${JSON.stringify(latest, null, 2)}`);
}

async function waitForMinimumAppCount(
  query: string,
  variables: JsonRecord,
  minimumCount: number,
  context: string,
): Promise<GraphqlResult> {
  let latest: GraphqlResult | null = null;
  for (let attempt = 1; attempt <= 20; attempt += 1) {
    latest = await runChecked(query, variables, `${context} attempt ${attempt}`);
    if (readCount(latest, 'appCount') >= minimumCount) {
      return latest;
    }
    await sleep(1_000);
  }
  throw new Error(`${context} did not reach minimum count ${minimumCount}: ${JSON.stringify(latest, null, 2)}`);
}

function captureRecordedCall(
  operationName: string,
  query: string,
  variables: JsonRecord,
  result: GraphqlResult,
): CaptureCall {
  return {
    operationName,
    variables,
    query,
    response: {
      status: result.status,
      body: result.payload,
    },
  };
}

function automaticAppInput(title: string, functionId: string, startsAt: string): JsonRecord {
  return {
    title,
    startsAt,
    functionId,
    discountClasses: ['ORDER'],
  };
}

const createDocument = await readRequest('discount-app-count-only-create.graphql');
const readDocument = await readRequest('discount-app-count-only-read.graphql');
const runId = readRunId();
const startsAt = new Date(Date.now() - 60_000).toISOString();
const queryFilter = 'type:app';

const functionCatalog = await runChecked(functionCatalogDocument, {}, 'discount app count-only function catalog');
const discountFunction = findDiscountFunction(readFunctionNodes(functionCatalog));
const functionId = requireStringPath(discountFunction, ['id'], 'discount Function');
const functionHydrate = await runChecked(
  functionHydrateByIdDocument,
  { id: functionId },
  'discount app count-only function hydrate',
);

const baselineTitle = `SDP app count baseline ${runId}`;
const localTitle = `SDP app count local ${runId}`;
const baselineCreateVariables: JsonRecord = {
  input: automaticAppInput(baselineTitle, functionId, startsAt),
};
const localCreateVariables: JsonRecord = {
  input: automaticAppInput(localTitle, functionId, startsAt),
};

const createdAutomaticIds: string[] = [];
const cleanupResults: GraphqlResult[] = [];

async function cleanupCreatedDiscounts(): Promise<void> {
  for (const id of createdAutomaticIds) {
    cleanupResults.push(await runGraphqlRaw<JsonRecord>(cleanupAutomaticDocument, { id }));
  }
}

let baselineCreate: GraphqlResult | null = null;
let baselineRead: GraphqlResult | null = null;
let localCreate: GraphqlResult | null = null;
let readAfterCreate: GraphqlResult | null = null;

try {
  baselineCreate = await runChecked(createDocument, baselineCreateVariables, 'discount app count-only baseline create');
  assertNoUserErrors(baselineCreate, 'discountAutomaticAppCreate', 'discount app count-only baseline create');
  const baselineId = requireStringPath(
    baselineCreate,
    ['payload', 'data', 'discountAutomaticAppCreate', 'automaticAppDiscount', 'discountId'],
    'baseline app discount id',
  );
  createdAutomaticIds.push(baselineId);

  const baselineReadVariables: JsonRecord = {
    query: queryFilter,
    limit: 1,
  };
  const baselineProbe = await waitForMinimumAppCount(
    readDocument,
    baselineReadVariables,
    1,
    'discount app count-only baseline read',
  );
  const baselineCount = readCount(baselineProbe, 'appCount');
  const readVariables: JsonRecord = {
    query: queryFilter,
    limit: Math.max(1, baselineCount),
  };
  baselineRead = await waitForCount(
    readDocument,
    readVariables,
    baselineCount,
    baselineCount,
    'AT_LEAST',
    'discount app count-only baseline read with final limit',
  );

  localCreate = await runChecked(
    createDocument,
    localCreateVariables,
    'discount app count-only local-equivalent create',
  );
  assertNoUserErrors(localCreate, 'discountAutomaticAppCreate', 'discount app count-only local-equivalent create');
  const localId = requireStringPath(
    localCreate,
    ['payload', 'data', 'discountAutomaticAppCreate', 'automaticAppDiscount', 'discountId'],
    'local-equivalent app discount id',
  );
  createdAutomaticIds.push(localId);

  readAfterCreate = await waitForCount(
    readDocument,
    readVariables,
    baselineCount + 1,
    baselineCount,
    'AT_LEAST',
    'discount app count-only read after create',
  );

  await cleanupCreatedDiscounts();

  const output = {
    scenarioId: 'discount-app-count-only-read-after-create',
    storeDomain,
    apiVersion,
    runId,
    variables: {
      query: queryFilter,
      limit: readVariables['limit'],
      startsAt,
      baselineTitle,
      localTitle,
      functionId,
      baselineCount,
      baselineId,
      localId,
    },
    requests: {
      functionHydrate: { query: functionHydrateByIdDocument, variables: { id: functionId } },
      baselineCreate: { query: createDocument, variables: baselineCreateVariables },
      baselineRead: { query: readDocument, variables: readVariables },
      localCreate: { query: createDocument, variables: localCreateVariables },
      readAfterCreate: { query: readDocument, variables: readVariables },
    },
    scopeProbe,
    setup: {
      functionCatalog,
      functionHydrate,
      baselineCreate,
      baselineRead,
    },
    localCreate,
    readAfterCreate,
    cleanup: cleanupResults,
    upstreamCalls: [
      captureRecordedCall('ShopifyFunctionById', functionHydrateByIdDocument, { id: functionId }, functionHydrate),
      captureRecordedCall('DiscountAppCountOnlyRead', readDocument, readVariables, baselineRead),
    ],
  };

  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        baselineCount,
        baselineId,
        localId,
      },
      null,
      2,
    ),
  );
} catch (error) {
  if (createdAutomaticIds.length > 0) {
    await cleanupCreatedDiscounts();
    console.error(`Cleaned up created discounts after capture failure: ${JSON.stringify(cleanupResults, null, 2)}`);
  }
  throw error;
}
