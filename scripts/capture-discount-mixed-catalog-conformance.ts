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
const outputPath = path.join(outputDir, 'discount-mixed-catalog.json');
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

function assertNoGraphqlErrors(result: GraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(result: GraphqlResult, rootName: string, label: string): void {
  assertNoGraphqlErrors(result, label);
  const errors = readArray(readPath(result, ['payload', 'data', rootName, 'userErrors']));
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

function discountTitle(node: unknown, discountKey: string): string {
  return requireStringPath(node, [discountKey, 'title'], `${discountKey} title`);
}

function adminDiscountTitle(node: unknown): string {
  return requireStringPath(node, ['discount', 'title'], 'discount title');
}

function catalogTitles(result: GraphqlResult, rootName: string): string[] {
  return readArray(readPath(result, ['payload', 'data', rootName, 'nodes'])).map(adminDiscountTitle);
}

function familyTitles(result: GraphqlResult, rootName: string, discountKey: string): string[] {
  return readArray(readPath(result, ['payload', 'data', rootName, 'nodes'])).map((node) =>
    discountTitle(node, discountKey),
  );
}

function assertStartsWith(label: string, actual: string[], expected: string[]): void {
  const window = actual.slice(0, expected.length);
  if (window.length !== expected.length || window.some((value, index) => value !== expected[index])) {
    throw new Error(`${label}: expected prefix ${JSON.stringify(expected)} got ${JSON.stringify(actual)}`);
  }
}

function assertCountAtLeast(result: GraphqlResult, expectedCount: number): void {
  const count = readPath(result, ['payload', 'data', 'limitedCount', 'count']);
  const precision = readPath(result, ['payload', 'data', 'limitedCount', 'precision']);
  if (count !== expectedCount || precision !== 'AT_LEAST') {
    throw new Error(`limitedCount mismatch: ${JSON.stringify(readPath(result, ['payload', 'data', 'limitedCount']))}`);
  }
}

function assertBaselineRead(result: GraphqlResult, titles: ExpectedTitles): void {
  assertStartsWith('baseline reverse discountNodes', catalogTitles(result, 'reverseCatalog'), [
    titles.upstreamCode,
    titles.upstreamAutomatic,
  ]);
  assertStartsWith('baseline forward discountNodes', catalogTitles(result, 'forwardCatalog'), [
    titles.upstreamAutomatic,
    titles.upstreamCode,
  ]);
  assertStartsWith('baseline codeDiscountNodes', familyTitles(result, 'codeCatalog', 'codeDiscount'), [
    titles.upstreamCode,
  ]);
  assertStartsWith('baseline automaticDiscountNodes', familyTitles(result, 'automaticCatalog', 'automaticDiscount'), [
    titles.upstreamAutomatic,
  ]);
}

function assertReadAfterCreate(result: GraphqlResult, titles: ExpectedTitles): void {
  assertStartsWith('mixed reverse discountNodes', catalogTitles(result, 'reverseCatalog'), [
    titles.upstreamCode,
    titles.localCode,
    titles.upstreamAutomatic,
  ]);
  assertStartsWith('mixed forward discountNodes', catalogTitles(result, 'forwardCatalog'), [
    titles.upstreamAutomatic,
    titles.localCode,
  ]);
  assertStartsWith('mixed codeDiscountNodes', familyTitles(result, 'codeCatalog', 'codeDiscount'), [
    titles.upstreamCode,
    titles.localCode,
  ]);
  assertStartsWith('mixed automaticDiscountNodes', familyTitles(result, 'automaticCatalog', 'automaticDiscount'), [
    titles.upstreamAutomatic,
  ]);
  assertCountAtLeast(result, 2);
}

function assertReadAfterMutations(result: GraphqlResult, titles: ExpectedTitles): void {
  assertStartsWith('post-mutation reverse discountNodes', catalogTitles(result, 'reverseCatalog'), [
    titles.localCode,
    titles.upstreamAutomaticUpdated,
  ]);
  assertStartsWith('post-mutation codeDiscountNodes', familyTitles(result, 'codeCatalog', 'codeDiscount'), [
    titles.localCode,
  ]);
  assertStartsWith(
    'post-mutation automaticDiscountNodes',
    familyTitles(result, 'automaticCatalog', 'automaticDiscount'),
    [titles.upstreamAutomaticUpdated],
  );
  const tombstonedByCode = readPath(result, ['payload', 'data', 'tombstonedByCode']);
  if (tombstonedByCode !== null) {
    throw new Error(`post-mutation code lookup should be null: ${JSON.stringify(tombstonedByCode, null, 2)}`);
  }
}

async function waitForRead(
  query: string,
  variables: JsonRecord,
  assertRead: (result: GraphqlResult, titles: ExpectedTitles) => void,
  titles: ExpectedTitles,
  label: string,
): Promise<GraphqlResult> {
  let lastResult: GraphqlResult | null = null;
  let lastError: unknown = null;
  for (let attempt = 0; attempt < 20; attempt += 1) {
    lastResult = await runChecked(query, variables, label);
    try {
      assertRead(lastResult, titles);
      return lastResult;
    } catch (error) {
      lastError = error;
      await sleep(750);
    }
  }
  throw new Error(`${label} did not converge: ${String(lastError)}; last=${JSON.stringify(lastResult, null, 2)}`);
}

type ExpectedTitles = {
  upstreamCode: string;
  localCode: string;
  upstreamAutomatic: string;
  upstreamAutomaticUpdated: string;
};

const runId = readRunId();
const titlePrefix = `zzzzzz SDP mixed ${runId}`;
const titles: ExpectedTitles = {
  upstreamCode: `${titlePrefix} Z upstream code`,
  localCode: `${titlePrefix} Y local code`,
  upstreamAutomatic: `${titlePrefix} X upstream automatic`,
  upstreamAutomaticUpdated: `${titlePrefix} X upstream automatic updated`,
};
const upstreamCode = `SDPMIXUP${runId}`;
const localCode = `SDPMIXLOCAL${runId}`;
const startsAt = new Date(Date.now() + 14 * 24 * 60 * 60 * 1000).toISOString();
const queryFilter = 'status:scheduled';
const first = 20;
const countLimit = 2;

const createDocument = await readRequest('discount-mixed-catalog-create.graphql');
const readDocument = await readRequest('discount-mixed-catalog-read.graphql');
const updateAutomaticDocument = await readRequest('discount-mixed-catalog-update-automatic.graphql');
const deleteCodeDocument = await readRequest('discount-mixed-catalog-delete-code.graphql');
const readAfterMutationsDocument = await readRequest('discount-mixed-catalog-read-after-mutations.graphql');
const uniquenessDocument = await readRequest('discount-uniqueness-check.graphql');

const createUpstreamCodeVariables: JsonRecord = {
  input: codeInput(titles.upstreamCode, upstreamCode, startsAt),
};
const createUpstreamAutomaticVariables: JsonRecord = {
  input: automaticInput(titles.upstreamAutomatic, startsAt),
};
const createLocalVariables: JsonRecord = {
  input: codeInput(titles.localCode, localCode, startsAt),
};
const updateAutomaticVariablesInput = automaticInput(titles.upstreamAutomaticUpdated, startsAt);

const createdCodeIds: string[] = [];
const deletedCodeIds = new Set<string>();
const createdAutomaticIds: string[] = [];
const cleanupResults: GraphqlResult[] = [];

async function cleanupCreatedDiscounts(): Promise<void> {
  for (const id of createdCodeIds) {
    if (deletedCodeIds.has(id)) continue;
    cleanupResults.push(await runGraphqlRaw<JsonRecord>(deleteCodeDocument, { id }));
    deletedCodeIds.add(id);
  }
  for (const id of createdAutomaticIds) {
    cleanupResults.push(
      await runGraphqlRaw<JsonRecord>(
        `#graphql
          mutation DiscountMixedCatalogCleanupAutomatic($id: ID!) {
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
        `,
        { id },
      ),
    );
  }
}

let upstreamCodeCreate: GraphqlResult | null = null;
let upstreamAutomaticCreate: GraphqlResult | null = null;
let uniquenessBeforeLocal: GraphqlResult | null = null;
let baselineRead: GraphqlResult | null = null;
let baselineReadForAfterMutations: GraphqlResult | null = null;
let localCreate: GraphqlResult | null = null;
let readAfterCreate: GraphqlResult | null = null;
let updateAutomatic: GraphqlResult | null = null;
let deleteCode: GraphqlResult | null = null;
let readAfterMutations: GraphqlResult | null = null;

try {
  upstreamCodeCreate = await runChecked(
    createDocument,
    createUpstreamCodeVariables,
    'discount mixed catalog upstream code create',
  );
  assertNoUserErrors(upstreamCodeCreate, 'discountCodeBasicCreate', 'discount mixed catalog upstream code create');
  const upstreamCodeId = requireStringPath(
    upstreamCodeCreate,
    ['payload', 'data', 'discountCodeBasicCreate', 'codeDiscountNode', 'id'],
    'upstream code discount id',
  );
  createdCodeIds.push(upstreamCodeId);

  upstreamAutomaticCreate = await runChecked(
    `#graphql
      mutation DiscountMixedCatalogCreateAutomatic($input: DiscountAutomaticBasicInput!) {
        discountAutomaticBasicCreate(automaticBasicDiscount: $input) {
          automaticDiscountNode {
            id
            automaticDiscount {
              __typename
              ... on DiscountAutomaticBasic {
                title
                status
              }
            }
          }
          userErrors {
            field
            message
            code
            extraInfo
          }
        }
      }
    `,
    createUpstreamAutomaticVariables,
    'discount mixed catalog upstream automatic create',
  );
  assertNoUserErrors(
    upstreamAutomaticCreate,
    'discountAutomaticBasicCreate',
    'discount mixed catalog upstream automatic create',
  );
  const upstreamAutomaticId = requireStringPath(
    upstreamAutomaticCreate,
    ['payload', 'data', 'discountAutomaticBasicCreate', 'automaticDiscountNode', 'id'],
    'upstream automatic discount id',
  );
  createdAutomaticIds.push(upstreamAutomaticId);

  const readVariables: JsonRecord = {
    query: queryFilter,
    first,
    countLimit,
    upstreamCode,
    upstreamAutomaticId,
  };
  const readAfterMutationsVariables: JsonRecord = {
    query: queryFilter,
    first,
    upstreamCode,
    upstreamAutomaticId,
  };
  const uniquenessVariables = { code: localCode };

  uniquenessBeforeLocal = await runChecked(
    uniquenessDocument,
    uniquenessVariables,
    'discount mixed catalog uniqueness before local create',
  );
  if (readPath(uniquenessBeforeLocal, ['payload', 'data', 'codeDiscountNodeByCode']) !== null) {
    throw new Error(`local code was unexpectedly taken: ${JSON.stringify(uniquenessBeforeLocal, null, 2)}`);
  }

  baselineRead = await waitForRead(
    readDocument,
    readVariables,
    assertBaselineRead,
    titles,
    'discount mixed catalog baseline read',
  );
  baselineReadForAfterMutations = await waitForRead(
    readAfterMutationsDocument,
    readAfterMutationsVariables,
    (result, expectedTitles) => {
      assertStartsWith(
        'baseline after-mutation hydrate reverse discountNodes',
        catalogTitles(result, 'reverseCatalog'),
        [expectedTitles.upstreamCode, expectedTitles.upstreamAutomatic],
      );
    },
    titles,
    'discount mixed catalog baseline hydrate for post-mutation read',
  );

  localCreate = await runChecked(createDocument, createLocalVariables, 'discount mixed catalog local code create');
  assertNoUserErrors(localCreate, 'discountCodeBasicCreate', 'discount mixed catalog local code create');
  const localCodeId = requireStringPath(
    localCreate,
    ['payload', 'data', 'discountCodeBasicCreate', 'codeDiscountNode', 'id'],
    'local code discount id',
  );
  createdCodeIds.push(localCodeId);

  readAfterCreate = await waitForRead(
    readDocument,
    readVariables,
    assertReadAfterCreate,
    titles,
    'discount mixed catalog read after local create',
  );

  const updateVariables: JsonRecord = {
    id: upstreamAutomaticId,
    input: updateAutomaticVariablesInput,
  };
  updateAutomatic = await runChecked(
    updateAutomaticDocument,
    updateVariables,
    'discount mixed catalog upstream automatic update',
  );
  assertNoUserErrors(
    updateAutomatic,
    'discountAutomaticBasicUpdate',
    'discount mixed catalog upstream automatic update',
  );

  deleteCode = await runChecked(
    deleteCodeDocument,
    { id: upstreamCodeId },
    'discount mixed catalog upstream code delete',
  );
  assertNoUserErrors(deleteCode, 'discountCodeDelete', 'discount mixed catalog upstream code delete');
  deletedCodeIds.add(upstreamCodeId);

  readAfterMutations = await waitForRead(
    readAfterMutationsDocument,
    readAfterMutationsVariables,
    assertReadAfterMutations,
    titles,
    'discount mixed catalog read after update and delete',
  );

  await cleanupCreatedDiscounts();

  const output = {
    scenarioId: 'discount-mixed-catalog',
    storeDomain,
    apiVersion,
    runId,
    variables: {
      query: queryFilter,
      first,
      countLimit,
      startsAt,
      upstreamCodeId,
      localCodeId,
      upstreamAutomaticId,
      ...titles,
    },
    requests: {
      upstreamCodeCreate: { query: createDocument, variables: createUpstreamCodeVariables },
      upstreamAutomaticCreate: { variables: createUpstreamAutomaticVariables },
      uniquenessBeforeLocal: { query: uniquenessDocument, variables: uniquenessVariables },
      baselineRead: { query: readDocument, variables: readVariables },
      baselineReadForAfterMutations: { query: readAfterMutationsDocument, variables: readAfterMutationsVariables },
      localCreate: { query: createDocument, variables: createLocalVariables },
      readAfterCreate: { query: readDocument, variables: readVariables },
      updateAutomatic: { query: updateAutomaticDocument, variables: updateVariables },
      deleteCode: { query: deleteCodeDocument, variables: { id: upstreamCodeId } },
      readAfterMutations: { query: readAfterMutationsDocument, variables: readAfterMutationsVariables },
    },
    scopeProbe,
    setup: {
      upstreamCodeCreate,
      upstreamAutomaticCreate,
      uniquenessBeforeLocal,
      baselineRead,
      baselineReadForAfterMutations,
    },
    localCreate,
    readAfterCreate,
    updateAutomatic,
    deleteCode,
    readAfterMutations,
    cleanup: cleanupResults,
    upstreamCalls: [
      {
        operationName: 'DiscountUniquenessCheck',
        variables: uniquenessVariables,
        query: uniquenessDocument,
        response: { status: uniquenessBeforeLocal.status, body: uniquenessBeforeLocal.payload },
      },
      {
        operationName: 'DiscountMixedCatalogRead',
        variables: readVariables,
        query: readDocument,
        response: { status: baselineRead.status, body: baselineRead.payload },
      },
      {
        operationName: 'DiscountMixedCatalogReadAfterMutations',
        variables: readAfterMutationsVariables,
        query: readAfterMutationsDocument,
        response: { status: baselineReadForAfterMutations.status, body: baselineReadForAfterMutations.payload },
      },
    ],
  };

  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        upstreamCodeId,
        localCodeId,
        upstreamAutomaticId,
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
