/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlResult = { status: number; payload: unknown };

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'marketing');
const outputPath = path.join(outputDir, 'marketing-live-hybrid-non-empty-read.json');
const readDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-live-hybrid-non-empty-read.graphql'),
  'utf8',
);
const readAfterDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-live-hybrid-non-empty-read-after.graphql'),
  'utf8',
);

const createDocument = `#graphql
  mutation MarketingLiveHybridReadSetup($input: MarketingActivityCreateExternalInput!) {
    created: marketingActivityCreateExternal(input: $input) {
      marketingActivity {
        id
        title
        status
        isExternal
        marketingEvent {
          id
          type
          remoteId
          description
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteByRemoteDocument = `#graphql
  mutation MarketingLiveHybridReadCleanup($remoteId: String) {
    marketingActivityDeleteExternal(remoteId: $remoteId) {
      deletedMarketingActivityId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function randomSuffix(): string {
  return `${Date.now()}${Math.random().toString(36).slice(2, 8)}`;
}

function readRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, parts: string[]): unknown {
  let current: unknown = value;
  for (const part of parts) {
    if (Array.isArray(current)) {
      const index = Number(part);
      current = Number.isInteger(index) ? current[index] : undefined;
    } else {
      current = readRecord(current)?.[part];
    }
    if (current === undefined) return undefined;
  }
  return current;
}

function readStringPath(payload: unknown, parts: string[], label: string): string {
  const value = readPath(payload, parts);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} missing string at ${parts.join('.')}: ${JSON.stringify(payload)}`);
  }
  return value;
}

async function assertGraphqlOk(label: string, result: GraphqlResult): Promise<void> {
  if (result.status >= 200 && result.status < 300 && !readRecord(result.payload)?.['errors']) {
    return;
  }
  console.error(JSON.stringify(result.payload, null, 2));
  throw new Error(`${label} failed with HTTP ${result.status}`);
}

function assertNoUserErrors(label: string, result: GraphqlResult): void {
  const userErrors = readPath(result.payload, ['data', 'created', 'userErrors']);
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
}

function assertNodeTitle(payload: unknown, parts: string[], expected: string, label: string): void {
  const actual = readStringPath(payload, parts, label);
  if (actual !== expected) {
    throw new Error(`${label} expected ${expected}, got ${actual}`);
  }
}

function buildInput(
  title: string,
  remoteId: string,
  tactic: 'AD' | 'NEWSLETTER',
  marketingChannelType: 'SEARCH' | 'EMAIL',
) {
  return {
    title,
    remoteId,
    status: 'ACTIVE',
    remoteUrl: `https://example.com/${remoteId}`,
    tactic,
    marketingChannelType,
    utm: {
      campaign: remoteId,
      source: marketingChannelType.toLowerCase(),
      medium: tactic.toLowerCase(),
    },
  };
}

const suffix = randomSuffix();
const alphaTitle = `Alpha live hybrid ${suffix}`;
const zuluTitle = `Zulu live hybrid ${suffix}`;
const alphaRemoteId = `marketing-live-hybrid-alpha-${suffix}`;
const zuluRemoteId = `marketing-live-hybrid-zulu-${suffix}`;
const alphaVariables = {
  input: buildInput(alphaTitle, alphaRemoteId, 'NEWSLETTER', 'EMAIL'),
};
const zuluVariables = {
  input: buildInput(zuluTitle, zuluRemoteId, 'AD', 'SEARCH'),
};

let createAlphaResponse: GraphqlResult | null = null;
let createZuluResponse: GraphqlResult | null = null;
let readResponse: GraphqlResult | null = null;
let readAfterResponse: GraphqlResult | null = null;
const cleanupResponses: Record<string, GraphqlResult> = {};
let readVariables: Record<string, unknown> | null = null;
let readAfterVariables: Record<string, unknown> | null = null;
let alphaId = '';
let zuluId = '';
let alphaEventId = '';
let zuluEventId = '';

try {
  createAlphaResponse = await runGraphqlRequest(createDocument, alphaVariables);
  await assertGraphqlOk('create-alpha', createAlphaResponse);
  assertNoUserErrors('create-alpha', createAlphaResponse);

  await sleep(1200);

  createZuluResponse = await runGraphqlRequest(createDocument, zuluVariables);
  await assertGraphqlOk('create-zulu', createZuluResponse);
  assertNoUserErrors('create-zulu', createZuluResponse);

  alphaId = readStringPath(createAlphaResponse.payload, ['data', 'created', 'marketingActivity', 'id'], 'alphaId');
  zuluId = readStringPath(createZuluResponse.payload, ['data', 'created', 'marketingActivity', 'id'], 'zuluId');
  alphaEventId = readStringPath(
    createAlphaResponse.payload,
    ['data', 'created', 'marketingActivity', 'marketingEvent', 'id'],
    'alphaEventId',
  );
  zuluEventId = readStringPath(
    createZuluResponse.payload,
    ['data', 'created', 'marketingActivity', 'marketingEvent', 'id'],
    'zuluEventId',
  );

  readVariables = {
    activityId: alphaId,
    eventId: alphaEventId,
    first: 2,
    remoteIds: [alphaRemoteId, zuluRemoteId],
    activityQuery: `title:"${alphaTitle}"`,
  };
  readResponse = await runGraphqlRequest(readDocument, readVariables);
  await assertGraphqlOk('read', readResponse);
  assertNodeTitle(readResponse.payload, ['data', 'allActivities', 'nodes', '0', 'title'], alphaTitle, 'all[0]');
  assertNodeTitle(readResponse.payload, ['data', 'allActivities', 'nodes', '1', 'title'], zuluTitle, 'all[1]');
  assertNodeTitle(readResponse.payload, ['data', 'selectedActivity', 'title'], alphaTitle, 'selectedActivity');
  assertNodeTitle(
    readResponse.payload,
    ['data', 'searchedActivities', 'nodes', '0', 'title'],
    alphaTitle,
    'searchedActivities',
  );
  readStringPath(readResponse.payload, ['data', 'selectedEvent', 'id'], 'selectedEvent');
  readStringPath(readResponse.payload, ['data', 'latestEvents', 'edges', '0', 'cursor'], 'latestEventCursor');

  const activityCursor = readStringPath(
    readResponse.payload,
    ['data', 'firstActivity', 'edges', '0', 'cursor'],
    'activityCursor',
  );
  readAfterVariables = {
    activityCursor,
    remoteIds: [alphaRemoteId, zuluRemoteId],
  };
  readAfterResponse = await runGraphqlRequest(readAfterDocument, readAfterVariables);
  await assertGraphqlOk('read-after', readAfterResponse);
  assertNodeTitle(
    readAfterResponse.payload,
    ['data', 'afterFirstActivity', 'nodes', '0', 'title'],
    zuluTitle,
    'afterFirstActivity',
  );
} finally {
  for (const [label, remoteId] of [
    ['alpha', alphaRemoteId],
    ['zulu', zuluRemoteId],
  ] as const) {
    cleanupResponses[label] = await runGraphqlRequest(deleteByRemoteDocument, { remoteId });
  }
}

if (!createAlphaResponse || !createZuluResponse || !readResponse || !readAfterResponse) {
  throw new Error('capture did not complete all required operations');
}
if (!readVariables || !readAfterVariables) {
  throw new Error('capture did not record read variables');
}

await mkdir(outputDir, { recursive: true });
const capture = {
  scenarioId: 'marketing-live-hybrid-non-empty-read',
  apiVersion,
  storeDomain,
  capturedAt: new Date().toISOString(),
  setup: {
    disposableRemoteIds: { alpha: alphaRemoteId, zulu: zuluRemoteId },
    activityIds: { alpha: alphaId, zulu: zuluId },
    eventIds: { alpha: alphaEventId, zulu: zuluEventId },
    titles: { alpha: alphaTitle, zulu: zuluTitle },
    cleanup: 'Deletes both disposable external marketing activities by remote ID.',
  },
  operations: {
    createAlpha: {
      request: { query: createDocument, variables: alphaVariables },
      response: createAlphaResponse,
    },
    createZulu: {
      request: { query: createDocument, variables: zuluVariables },
      response: createZuluResponse,
    },
    read: {
      request: { query: readDocument, variables: readVariables },
      response: readResponse,
    },
    readAfter: {
      request: { query: readAfterDocument, variables: readAfterVariables },
      response: readAfterResponse,
    },
  },
  cleanup: cleanupResponses,
  upstreamCalls: [
    {
      operationName: 'MarketingLiveHybridNonEmptyRead',
      variables: readVariables,
      query: readDocument,
      response: { status: readResponse.status, body: readResponse.payload },
    },
    {
      operationName: 'MarketingLiveHybridNonEmptyReadAfter',
      variables: readAfterVariables,
      query: readAfterDocument,
      response: { status: readAfterResponse.status, body: readAfterResponse.payload },
    },
  ],
};
await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`);
console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      apiVersion,
      storeDomain,
      activityIds: { alpha: alphaId, zulu: zuluId },
      eventIds: { alpha: alphaEventId, zulu: zuluEventId },
      upstreamCallCount: capture.upstreamCalls.length,
    },
    null,
    2,
  ),
);
