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
const outputPath = path.join(outputDir, 'marketing-activity-connection-window.json');

const createDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-connection-window-create.graphql'),
  'utf8',
);
const readFirstDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-connection-window-read-first.graphql'),
  'utf8',
);
const readAfterDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-connection-window-read-after.graphql'),
  'utf8',
);

const deleteByRemoteDocument = `#graphql
  mutation MarketingActivityConnectionWindowCleanup($remoteId: String) {
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
      const record = readRecord(current);
      current = record?.[part];
    }
    if (current === undefined) return undefined;
  }
  return current;
}

function readStringPath(payload: unknown, parts: string[], label: string): string {
  const value = readPath(payload, parts);
  if (typeof value !== 'string') {
    throw new Error(`${label} missing string at ${parts.join('.')}: ${JSON.stringify(payload)}`);
  }
  return value;
}

function readBoolPath(payload: unknown, parts: string[], label: string): boolean {
  const value = readPath(payload, parts);
  if (typeof value !== 'boolean') {
    throw new Error(`${label} missing boolean at ${parts.join('.')}: ${JSON.stringify(payload)}`);
  }
  return value;
}

function readUserErrors(payload: unknown): unknown[] {
  const value = readPath(payload, ['data', 'created', 'userErrors']);
  return Array.isArray(value) ? value : [];
}

async function assertGraphqlOk(label: string, result: GraphqlResult): Promise<void> {
  if (result.status >= 200 && result.status < 300 && !readRecord(result.payload)?.['errors']) {
    return;
  }
  console.error(JSON.stringify(result.payload, null, 2));
  throw new Error(`${label} failed with HTTP ${result.status}`);
}

function assertNoUserErrors(label: string, result: GraphqlResult): void {
  const userErrors = readUserErrors(result.payload);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function resourceTail(id: string): string {
  return id.slice(id.lastIndexOf('/') + 1);
}

function assertNodeTitle(payload: unknown, pathParts: string[], expected: string, label: string): void {
  const actual = readStringPath(payload, pathParts, label);
  if (actual !== expected) {
    throw new Error(`${label} expected ${expected}, got ${actual}`);
  }
}

const suffix = randomSuffix();
const searchToken = `connwindow${suffix}`;
const alphaTitle = `Alpha${searchToken}`;
const zuluTitle = `Zulu${searchToken}`;
const alphaRemoteId = `marketing-connection-alpha-${suffix}`;
const zuluRemoteId = `marketing-connection-zulu-${suffix}`;
const alphaScheduledEnd = '2026-05-31T00:00:00Z';
const zuluScheduledEnd = '2026-06-01T00:00:00Z';

const alphaVariables = {
  input: {
    title: alphaTitle,
    remoteId: alphaRemoteId,
    status: 'ACTIVE',
    remoteUrl: `https://example.com/${alphaRemoteId}`,
    tactic: 'NEWSLETTER',
    marketingChannelType: 'EMAIL',
    scheduledEnd: alphaScheduledEnd,
    utm: { campaign: alphaRemoteId, source: 'email', medium: 'newsletter' },
  },
};
const zuluVariables = {
  input: {
    title: zuluTitle,
    remoteId: zuluRemoteId,
    status: 'PAUSED',
    remoteUrl: `https://example.com/${zuluRemoteId}`,
    tactic: 'AD',
    marketingChannelType: 'SEARCH',
    scheduledEnd: zuluScheduledEnd,
    utm: { campaign: zuluRemoteId, source: 'search', medium: 'ad' },
  },
};

let createAlphaResponse: GraphqlResult | null = null;
let createZuluResponse: GraphqlResult | null = null;
let readFirstResponse: GraphqlResult | null = null;
let readAfterResponse: GraphqlResult | null = null;
const cleanupResponses: Record<string, GraphqlResult> = {};
let alphaId = '';
let zuluId = '';
let readFirstVariables: Record<string, unknown> | null = null;
let readAfterVariables: Record<string, unknown> | null = null;

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
  const zuluCreatedAt = readStringPath(
    createZuluResponse.payload,
    ['data', 'created', 'marketingActivity', 'createdAt'],
    'zuluCreatedAt',
  );
  readFirstVariables = {
    first: 1,
    activityQuery: `title:${zuluTitle}`,
    titleQuery: `title:${zuluTitle}`,
    createdAtQuery: `created_at:">=${zuluCreatedAt}" title:${zuluTitle}`,
    idRangeQuery: `id:>${resourceTail(alphaId)} title:${zuluTitle}`,
    scheduledEndQuery: `scheduled_to_end_at:"${zuluScheduledEnd}" title:${zuluTitle}`,
    booleanOrQuery: `title:${alphaTitle} OR title:${zuluTitle}`,
    booleanAndQuery: `title:${alphaTitle} AND scheduled_to_end_at:"${alphaScheduledEnd}"`,
  };

  readFirstResponse = await runGraphqlRequest(readFirstDocument, readFirstVariables);
  await assertGraphqlOk('read-first', readFirstResponse);
  assertNodeTitle(readFirstResponse.payload, ['data', 'latestActivity', 'nodes', '0', 'title'], zuluTitle, 'latest');
  if (
    !readBoolPath(readFirstResponse.payload, ['data', 'latestActivity', 'pageInfo', 'hasNextPage'], 'latest hasNext')
  ) {
    throw new Error('latestActivity should report hasNextPage');
  }
  assertNodeTitle(readFirstResponse.payload, ['data', 'titleSearch', 'nodes', '0', 'title'], zuluTitle, 'titleSearch');
  assertNodeTitle(readFirstResponse.payload, ['data', 'titleFilter', 'nodes', '0', 'title'], zuluTitle, 'titleFilter');
  assertNodeTitle(
    readFirstResponse.payload,
    ['data', 'createdAtFilter', 'nodes', '0', 'title'],
    zuluTitle,
    'createdAtFilter',
  );
  assertNodeTitle(
    readFirstResponse.payload,
    ['data', 'idRangeFilter', 'nodes', '0', 'title'],
    zuluTitle,
    'idRangeFilter',
  );
  assertNodeTitle(
    readFirstResponse.payload,
    ['data', 'scheduledEndFilter', 'nodes', '0', 'title'],
    zuluTitle,
    'scheduledEndFilter',
  );
  assertNodeTitle(readFirstResponse.payload, ['data', 'booleanOr', 'nodes', '0', 'title'], alphaTitle, 'booleanOr[0]');
  assertNodeTitle(readFirstResponse.payload, ['data', 'booleanOr', 'nodes', '1', 'title'], zuluTitle, 'booleanOr[1]');
  assertNodeTitle(readFirstResponse.payload, ['data', 'booleanAnd', 'nodes', '0', 'title'], alphaTitle, 'booleanAnd');
  const latestEventType = readStringPath(
    readFirstResponse.payload,
    ['data', 'latestEvent', 'nodes', '0', 'type'],
    'latestEvent',
  );
  if (latestEventType !== 'AD') {
    throw new Error(`latestEvent expected AD, got ${latestEventType}`);
  }

  const activityCursor = readStringPath(
    readFirstResponse.payload,
    ['data', 'latestActivity', 'edges', '0', 'cursor'],
    'activityCursor',
  );
  readAfterVariables = { activityCursor };
  readAfterResponse = await runGraphqlRequest(readAfterDocument, readAfterVariables);
  await assertGraphqlOk('read-after', readAfterResponse);
  assertNodeTitle(readAfterResponse.payload, ['data', 'afterLatest', 'nodes', '0', 'title'], alphaTitle, 'afterLatest');
  if (
    !readBoolPath(
      readAfterResponse.payload,
      ['data', 'afterLatest', 'pageInfo', 'hasPreviousPage'],
      'after hasPrevious',
    )
  ) {
    throw new Error('afterLatest should report hasPreviousPage');
  }
} finally {
  for (const [label, remoteId] of [
    ['alpha', alphaRemoteId],
    ['zulu', zuluRemoteId],
  ] as const) {
    cleanupResponses[label] = await runGraphqlRequest(deleteByRemoteDocument, { remoteId });
  }
}

if (!createAlphaResponse || !createZuluResponse || !readFirstResponse || !readAfterResponse) {
  throw new Error('capture did not complete all required operations');
}
if (!readFirstVariables || !readAfterVariables) {
  throw new Error('capture did not record read variables');
}

await mkdir(outputDir, { recursive: true });
const capture = {
  scenarioId: 'marketing-activity-connection-window',
  apiVersion,
  storeDomain,
  capturedAt: new Date().toISOString(),
  setup: {
    disposableRemoteIds: { alpha: alphaRemoteId, zulu: zuluRemoteId },
    activityIds: { alpha: alphaId, zulu: zuluId },
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
    readFirst: {
      request: { query: readFirstDocument, variables: readFirstVariables },
      response: readFirstResponse,
    },
    readAfter: {
      request: { query: readAfterDocument, variables: readAfterVariables },
      response: readAfterResponse,
    },
  },
  cleanup: cleanupResponses,
};
await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`);
console.log(`wrote ${outputPath}`);
