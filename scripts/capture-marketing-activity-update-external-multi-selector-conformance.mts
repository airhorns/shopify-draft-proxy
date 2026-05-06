/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlResult = { status: number; payload: unknown };

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'marketing');
const outputPath = path.join(outputDir, 'marketing-activity-update-external-multi-selector.json');

const primaryDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-update-external-multi-selector.graphql'),
  'utf8',
);
const readDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-update-external-multi-selector-read.graphql'),
  'utf8',
);

const deleteDocument = `#graphql
  mutation MarketingActivityUpdateExternalMultiSelectorCleanup($remoteId: String) {
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
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function readRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, parts: string[]): unknown {
  let current: unknown = value;
  for (const part of parts) {
    const record = readRecord(current);
    if (!record) return undefined;
    current = record[part];
  }
  return current;
}

function readUserErrors(payload: unknown, root: string): unknown[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value : [];
}

function readFirstUserErrorCode(payload: unknown, root: string): string | null {
  const [first] = readUserErrors(payload, root);
  const code = readRecord(first)?.['code'];
  return typeof code === 'string' ? code : null;
}

async function assertGraphqlOk(label: string, result: GraphqlResult): Promise<void> {
  if (result.status >= 200 && result.status < 300 && !readRecord(result.payload)?.['errors']) {
    return;
  }
  console.error(JSON.stringify(result.payload, null, 2));
  throw new Error(`${label} failed with HTTP ${result.status}`);
}

function assertNoUserErrors(label: string, payload: unknown, root: string): void {
  const userErrors = readUserErrors(payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function assertUserErrorCode(label: string, payload: unknown, root: string, expectedCode: string): void {
  const actual = readFirstUserErrorCode(payload, root);
  if (actual !== expectedCode) {
    throw new Error(`${label} expected ${expectedCode}, got ${actual ?? '<none>'}: ${JSON.stringify(payload)}`);
  }
}

function readStringPath(payload: unknown, parts: string[], label: string): string {
  const value = readPath(payload, parts);
  if (typeof value !== 'string') {
    throw new Error(`${label} missing string at ${parts.join('.')}: ${JSON.stringify(payload)}`);
  }
  return value;
}

const suffix = randomSuffix();
const activityARemoteId = `multi-selector-a-${suffix}`;
const activityBRemoteId = `multi-selector-b-${suffix}`;
const activityAUtm = { campaign: `multi-selector-camp-a-${suffix}`, source: 'email', medium: 'newsletter' };
const activityBUtm = { campaign: `multi-selector-camp-b-${suffix}`, source: 'email', medium: 'newsletter' };
const activityATitle = `Multi selector A ${suffix}`;

function activityInput(kind: 'a' | 'b') {
  const isA = kind === 'a';
  return {
    title: isA ? activityATitle : `Multi selector B ${suffix}`,
    remoteId: isA ? activityARemoteId : activityBRemoteId,
    status: 'ACTIVE',
    remoteUrl: `https://example.com/multi-selector-${kind}`,
    tactic: 'NEWSLETTER',
    marketingChannelType: 'EMAIL',
    utm: isA ? activityAUtm : activityBUtm,
  };
}

const primaryVariables = {
  activityA: activityInput('a'),
  activityB: activityInput('b'),
  conflictRemoteId: activityARemoteId,
  conflictUtm: activityBUtm,
  updateInput: {
    title: `Should not stage ${suffix}`,
  },
};

const cleanupResponses: Record<string, unknown> = {};
const primaryResponse = await runGraphqlRequest(primaryDocument, primaryVariables);
await assertGraphqlOk('primary', primaryResponse);
assertNoUserErrors('create-a', primaryResponse.payload, 'createA');
assertNoUserErrors('create-b', primaryResponse.payload, 'createB');
assertUserErrorCode('conflict-update', primaryResponse.payload, 'conflictUpdate', 'MARKETING_ACTIVITY_DOES_NOT_EXIST');

const activityAId = readStringPath(
  primaryResponse.payload,
  ['data', 'createA', 'marketingActivity', 'id'],
  'activityAId',
);
const readVariables = { activityId: activityAId };
const readResponse = await runGraphqlRequest(readDocument, readVariables);
await assertGraphqlOk('read-activity-a', readResponse);
const readTitle = readStringPath(readResponse.payload, ['data', 'marketingActivity', 'title'], 'readTitle');
if (readTitle !== activityATitle) {
  throw new Error(`expected activity A title to remain ${activityATitle}, got ${readTitle}`);
}

for (const remoteId of [activityARemoteId, activityBRemoteId]) {
  cleanupResponses[remoteId] = await runGraphqlRequest(deleteDocument, { remoteId });
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'marketing-activity-update-external-multi-selector',
      apiVersion,
      storeDomain,
      capturedAt: new Date().toISOString(),
      operations: {
        primary: {
          query: primaryDocument,
          variables: primaryVariables,
          response: primaryResponse,
        },
        readActivityA: {
          query: readDocument,
          variables: readVariables,
          response: readResponse,
        },
      },
      cleanup: cleanupResponses,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      cases: ['primary', 'readActivityA'],
      cleanupRemoteIds: [activityARemoteId, activityBRemoteId],
    },
    null,
    2,
  ),
);
