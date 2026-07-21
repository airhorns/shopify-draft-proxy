/* oxlint-disable no-console -- CLI capture scripts intentionally report status. */
import 'dotenv/config';

import { randomBytes } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';
import { captureGiftCardCreateConfiguration } from './support/shopify/runtime-hydration-capture.js';

type JsonObject = Record<string, unknown>;
type CapturedStep = {
  operationName: string;
  query: string;
  variables: JsonObject;
  response: { status: number; body: unknown };
};

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

const requestDir = path.join('config', 'parity-requests', 'gift-cards');
const createDocument = await readFile(path.join(requestDir, 'gift-card-live-hybrid-overlay-create.graphql'), 'utf8');
const readDocument = await readFile(path.join(requestDir, 'gift-card-live-hybrid-overlay-read.graphql'), 'utf8');
const updateDocument = await readFile(path.join(requestDir, 'gift-card-live-hybrid-overlay-update.graphql'), 'utf8');
const deactivateDocument = await readFile(
  path.join(requestDir, 'gift-card-live-hybrid-overlay-deactivate.graphql'),
  'utf8',
);
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'gift-cards');
const outputPath = path.join(outputDir, 'gift-card-live-hybrid-overlay-windowing.json');

const baseCreateDocument = `mutation GiftCardLiveHybridOverlayBaseCreate(
  $first: GiftCardCreateInput!
  $second: GiftCardCreateInput!
  $third: GiftCardCreateInput!
  $fourth: GiftCardCreateInput!
) {
  first: giftCardCreate(input: $first) {
    giftCard { id lastCharacters enabled expiresOn initialValue { amount currencyCode } balance { amount currencyCode } }
    userErrors { field message }
  }
  second: giftCardCreate(input: $second) {
    giftCard { id lastCharacters enabled expiresOn initialValue { amount currencyCode } balance { amount currencyCode } }
    userErrors { field message }
  }
  third: giftCardCreate(input: $third) {
    giftCard { id lastCharacters enabled expiresOn initialValue { amount currencyCode } balance { amount currencyCode } }
    userErrors { field message }
  }
  fourth: giftCardCreate(input: $fourth) {
    giftCard { id lastCharacters enabled expiresOn initialValue { amount currencyCode } balance { amount currencyCode } }
    userErrors { field message }
  }
}
`;

function asObject(value: unknown, label: string): JsonObject {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${label} was not an object.`);
  }
  return value as JsonObject;
}

function asArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${label} was not an array.`);
  }
  return value;
}

function assertGraphqlOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

async function captureStep(operationName: string, query: string, variables: JsonObject): Promise<CapturedStep> {
  const result = await runGraphqlRequest(query, variables);
  assertGraphqlOk(operationName, result);
  return {
    operationName,
    query,
    variables,
    response: { status: result.status, body: result.payload },
  };
}

function responseData(step: CapturedStep): JsonObject {
  return asObject(
    asObject(step.response.body, `${step.operationName}.response.body`)['data'],
    `${step.operationName}.data`,
  );
}

function root(step: CapturedStep, name: string): JsonObject {
  return asObject(responseData(step)[name], `${step.operationName}.data.${name}`);
}

function assertEmptyUserErrors(step: CapturedStep, names: string[]): void {
  for (const name of names) {
    const userErrors = root(step, name)['userErrors'];
    if (!Array.isArray(userErrors) || userErrors.length !== 0) {
      throw new Error(`${step.operationName}.${name} returned userErrors: ${JSON.stringify(userErrors)}`);
    }
  }
}

function giftCardId(step: CapturedStep, name: string): string {
  const giftCard = asObject(root(step, name)['giftCard'], `${step.operationName}.${name}.giftCard`);
  const id = giftCard['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${step.operationName}.${name} did not return a gift-card id.`);
  }
  return id;
}

function lastCharactersFromConnection(step: CapturedStep, name: string): string[] {
  return asArray(root(step, name)['nodes'], `${step.operationName}.${name}.nodes`).map((node, index) => {
    const lastCharacters = asObject(node, `${step.operationName}.${name}.nodes[${index}]`)['lastCharacters'];
    if (typeof lastCharacters !== 'string') {
      throw new Error(`${step.operationName}.${name}.nodes[${index}].lastCharacters was not a string.`);
    }
    return lastCharacters;
  });
}

function hasNextPage(step: CapturedStep, name: string): boolean {
  const pageInfo = asObject(root(step, name)['pageInfo'], `${step.operationName}.${name}.pageInfo`);
  const value = pageInfo['hasNextPage'];
  if (typeof value !== 'boolean') {
    throw new Error(`${step.operationName}.${name}.pageInfo.hasNextPage was not a boolean.`);
  }
  return value;
}

function connectionMatches(
  step: CapturedStep,
  expectedForward: string[],
  expectedReverse: string[],
  expectedHasNextPage: boolean,
): boolean {
  return (
    lastCharactersFromConnection(step, 'forward').join('|') === expectedForward.join('|') &&
    lastCharactersFromConnection(step, 'reverse').join('|') === expectedReverse.join('|') &&
    hasNextPage(step, 'forward') === expectedHasNextPage
  );
}

async function captureUntil(
  operationName: string,
  variables: JsonObject,
  expectedForward: string[],
  expectedReverse: string[],
  expectedHasNextPage: boolean,
): Promise<CapturedStep> {
  let lastStep: CapturedStep | null = null;
  for (let attempt = 1; attempt <= 20; attempt += 1) {
    lastStep = await captureStep(operationName, readDocument, variables);
    if (connectionMatches(lastStep, expectedForward, expectedReverse, expectedHasNextPage)) {
      return lastStep;
    }
    await sleep(1_500);
  }
  throw new Error(
    `${operationName} did not observe the expected indexed window: ${JSON.stringify(lastStep?.response.body ?? null)}`,
  );
}

function upstreamCall(step: CapturedStep): JsonObject {
  return {
    method: 'POST',
    apiSurface: 'admin',
    apiVersion,
    path: `/admin/api/${apiVersion}/graphql.json`,
    operationName: step.operationName,
    variables: step.variables,
    query: step.query,
    response: step.response,
  };
}

async function deactivateForCleanup(id: string | null): Promise<unknown> {
  if (!id) return null;
  try {
    const result = await runGraphqlRequest(deactivateDocument, { id });
    return { id, status: result.status, body: result.payload };
  } catch (error) {
    return { id, error: error instanceof Error ? error.message : String(error) };
  }
}

let baseIds: string[] = [];
let stagedEquivalentId: string | null = null;
let liveDeactivatedId: string | null = null;

try {
  let searchToken = '';
  for (let attempt = 0; attempt < 16; attempt += 1) {
    const candidate = randomBytes(2).toString('hex').slice(0, 3).toUpperCase();
    const baseline = await captureStep('GiftCardLiveHybridOverlayUnusedTokenProbe', readDocument, {
      query: candidate,
      sortKey: 'INITIAL_VALUE',
    });
    if (lastCharactersFromConnection(baseline, 'forward').length === 0) {
      searchToken = candidate;
      break;
    }
  }
  if (!searchToken) {
    throw new Error('Could not find an unused gift-card search token.');
  }

  const runToken = Date.now().toString(36).toUpperCase();
  const code = (suffix: string): string => `OV${runToken}${searchToken}${suffix}`;
  const last = (suffix: string): string => `${searchToken}${suffix}`.toLowerCase();
  const baseCreateVariables = {
    first: {
      code: code('A'),
      initialValue: '10.01',
      expiresOn: '2028-01-01',
      note: 'LiveHybrid overlay authoritative A',
    },
    second: {
      code: code('B'),
      initialValue: '20.02',
      expiresOn: '2028-01-03',
      note: 'LiveHybrid overlay authoritative B',
    },
    third: {
      code: code('C'),
      initialValue: '30.03',
      expiresOn: '2028-01-05',
      note: 'LiveHybrid overlay authoritative C',
    },
    fourth: {
      code: code('D'),
      initialValue: '40.04',
      expiresOn: '2028-01-07',
      note: 'LiveHybrid overlay authoritative D',
    },
  };
  const baseCreate = await captureStep('GiftCardLiveHybridOverlayBaseCreate', baseCreateDocument, baseCreateVariables);
  assertEmptyUserErrors(baseCreate, ['first', 'second', 'third', 'fourth']);
  baseIds = ['first', 'second', 'third', 'fourth'].map((name) => giftCardId(baseCreate, name));
  const firstBaseId = baseIds[0];
  const secondBaseId = baseIds[1];
  if (!firstBaseId || !secondBaseId) {
    throw new Error('Gift-card overlay setup did not return the first two authoritative ids.');
  }

  const baseLast = ['A', 'B', 'C', 'D'].map(last);
  const initialValueReadVariables = { query: searchToken, sortKey: 'INITIAL_VALUE' };
  const expiresOnReadVariables = { query: searchToken, sortKey: 'EXPIRES_ON' };
  const enabledReadVariables = { query: `${searchToken} AND status:enabled`, sortKey: 'INITIAL_VALUE' };
  const initialValueBase = await captureUntil(
    'GiftCardLiveHybridOverlayRead',
    initialValueReadVariables,
    baseLast,
    [...baseLast].reverse().slice(0, 2),
    false,
  );
  const expiresOnBase = await captureUntil(
    'GiftCardLiveHybridOverlayRead',
    expiresOnReadVariables,
    baseLast,
    [...baseLast].reverse().slice(0, 2),
    false,
  );
  const enabledBase = await captureUntil(
    'GiftCardLiveHybridOverlayRead',
    enabledReadVariables,
    baseLast,
    [...baseLast].reverse().slice(0, 2),
    false,
  );
  const configuration = await captureGiftCardCreateConfiguration((query, variables) =>
    runGraphqlRequest(query, variables),
  );

  const createVariables = {
    code: code('E'),
    initialValue: '25.05',
    expiresOn: '2028-01-04',
  };
  const liveStagedCreate = await captureStep('GiftCardLiveHybridOverlayCreate', createDocument, createVariables);
  assertEmptyUserErrors(liveStagedCreate, ['giftCardCreate']);
  stagedEquivalentId = giftCardId(liveStagedCreate, 'giftCardCreate');
  const afterCreateRead = await captureUntil(
    'GiftCardLiveHybridOverlayRead',
    initialValueReadVariables,
    [last('A'), last('B'), last('E'), last('C')],
    [last('D'), last('C')],
    true,
  );

  const updateVariables = { id: firstBaseId, expiresOn: '2028-01-06' };
  const liveSortChangingUpdate = await captureStep('GiftCardLiveHybridOverlayUpdate', updateDocument, updateVariables);
  assertEmptyUserErrors(liveSortChangingUpdate, ['giftCardUpdate']);
  const afterSortChangingUpdateRead = await captureUntil(
    'GiftCardLiveHybridOverlayRead',
    expiresOnReadVariables,
    [last('B'), last('E'), last('C'), last('A')],
    [last('D'), last('A')],
    true,
  );

  const deactivateVariables = { id: secondBaseId };
  const liveDeactivate = await captureStep(
    'GiftCardLiveHybridOverlayDeactivate',
    deactivateDocument,
    deactivateVariables,
  );
  assertEmptyUserErrors(liveDeactivate, ['giftCardDeactivate']);
  liveDeactivatedId = secondBaseId;
  const afterEnabledRemovalRead = await captureUntil(
    'GiftCardLiveHybridOverlayRead',
    enabledReadVariables,
    [last('A'), last('E'), last('C'), last('D')],
    [last('D'), last('C')],
    false,
  );

  const cleanup = await Promise.all(
    [stagedEquivalentId, ...baseIds.filter((id) => id !== liveDeactivatedId)].map(deactivateForCleanup),
  );
  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId: 'gift-card-live-hybrid-overlay-windowing',
        storeDomain,
        apiVersion,
        proxyVariables: {
          create: createVariables,
          afterCreateRead: initialValueReadVariables,
          update: updateVariables,
          afterSortChangingUpdateRead: expiresOnReadVariables,
          deactivate: deactivateVariables,
          afterEnabledRemovalRead: enabledReadVariables,
        },
        setup: { baseCreate, initialValueBase, expiresOnBase, enabledBase },
        liveStagedCreate,
        afterCreateRead,
        liveSortChangingUpdate,
        afterSortChangingUpdateRead,
        liveDeactivate,
        afterEnabledRemovalRead,
        cleanup,
        upstreamCalls: [
          {
            method: 'POST',
            apiSurface: 'admin',
            apiVersion,
            path: `/admin/api/${apiVersion}/graphql.json`,
            ...configuration,
          },
          upstreamCall(initialValueBase),
          upstreamCall(expiresOnBase),
          upstreamCall(enabledBase),
        ],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} catch (error) {
  const cleanup = await Promise.all(
    [stagedEquivalentId, ...baseIds.filter((id) => id !== liveDeactivatedId)].map(deactivateForCleanup),
  );
  console.error(JSON.stringify({ ok: false, cleanup }, null, 2));
  throw error;
}
