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
const outputPath = path.join(outputDir, 'marketing-activity-create-external-read-after-write.json');

const primaryDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-create-external-read-after-write.graphql'),
  'utf8',
);
const readDocument = await readFile(
  path.join(
    'config',
    'parity-requests',
    'marketing',
    'marketing-activity-create-external-read-after-write-read.graphql',
  ),
  'utf8',
);

const deleteByRemoteDocument = `#graphql
  mutation MarketingActivityReadAfterWriteCleanupByRemote($remoteId: String) {
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

const deleteByIdDocument = `#graphql
  mutation MarketingActivityReadAfterWriteCleanupById($marketingActivityId: ID) {
    marketingActivityDeleteExternal(marketingActivityId: $marketingActivityId) {
      deletedMarketingActivityId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const appContextDocument = `#graphql
  query MarketingActivityReadAfterWriteAppContext {
    currentAppInstallation {
      app {
        id
        handle
        title
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

function assertNoUserErrors(label: string, payload: unknown, root: string): void {
  const userErrors = readUserErrors(payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

async function assertGraphqlOk(label: string, result: GraphqlResult): Promise<void> {
  if (result.status >= 200 && result.status < 300 && !readRecord(result.payload)?.['errors']) {
    return;
  }
  console.error(JSON.stringify(result.payload, null, 2));
  throw new Error(`${label} failed with HTTP ${result.status}`);
}

function readStringPath(payload: unknown, parts: string[], label: string): string {
  const value = readPath(payload, parts);
  if (typeof value !== 'string') {
    throw new Error(`${label} missing string at ${parts.join('.')}: ${JSON.stringify(payload)}`);
  }
  return value;
}

function readAppContext(payload: unknown): Record<string, string> {
  const app = readRecord(readPath(payload, ['data', 'currentAppInstallation', 'app']));
  if (!app) {
    throw new Error(`current app context missing app: ${JSON.stringify(payload)}`);
  }
  const id = readStringPath(payload, ['data', 'currentAppInstallation', 'app', 'id'], 'appContext.id');
  const title = readStringPath(payload, ['data', 'currentAppInstallation', 'app', 'title'], 'appContext.title');
  const handle = readStringPath(payload, ['data', 'currentAppInstallation', 'app', 'handle'], 'appContext.handle');
  const idTail = id.split('/').pop();
  if (title === idTail) {
    throw new Error(`current app title unexpectedly matched numeric app id tail ${idTail}`);
  }
  return { id, title, handle };
}

function assertActivityAppMatches(label: string, value: unknown, expectedApp: Record<string, string>): void {
  const app = readRecord(readPath(value, ['app']));
  if (!app) {
    throw new Error(`${label} missing app object: ${JSON.stringify(value)}`);
  }
  const expected = { id: expectedApp.id, title: expectedApp.title };
  const actual = {
    id: app.id,
    title: app.title,
  };
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${label}.app expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

function assertRoundTripFields(label: string, value: unknown): void {
  const record = readRecord(value);
  if (!record) {
    throw new Error(`${label} missing activity object: ${JSON.stringify(value)}`);
  }
  const expected = {
    adSpend: { amount: '25.0', currencyCode: 'USD' },
    marketingEvent: { scheduledToEndAt: '2026-05-31T00:00:00Z' },
  };
  for (const [key, expectedValue] of Object.entries(expected)) {
    const actual = record[key];
    if (JSON.stringify(actual) !== JSON.stringify(expectedValue)) {
      throw new Error(`${label}.${key} expected ${JSON.stringify(expectedValue)}, got ${JSON.stringify(actual)}`);
    }
  }
}

const suffix = randomSuffix();
const remoteId = `marketing-read-after-write-${suffix}`;
const createdTitle = `Marketing read after write ${suffix}`;
const updatedTitle = `Marketing read after write updated ${suffix}`;
const cleanupResponses: Record<string, unknown> = {};
let primaryResponse: GraphqlResult | null = null;
let readAfterUpdateResponse: GraphqlResult | null = null;
let activityId: string | null = null;

const appContextResponse = await runGraphqlRequest(appContextDocument);
await assertGraphqlOk('app-context', appContextResponse);
const appContext = readAppContext(appContextResponse.payload);

const primaryVariables = {
  remoteId,
  createInput: {
    title: createdTitle,
    remoteId,
    status: 'ACTIVE',
    remoteUrl: `https://example.com/${remoteId}`,
    tactic: 'AD',
    marketingChannelType: 'SEARCH',
    utm: { campaign: remoteId, source: 'ads', medium: 'cpc' },
    budget: { budgetType: 'DAILY', total: { amount: '100.00', currencyCode: 'USD' } },
    adSpend: { amount: '25.00', currencyCode: 'USD' },
    scheduledStart: '2026-05-01T00:00:00Z',
    scheduledEnd: '2026-05-31T00:00:00Z',
    referringDomain: 'ads.example.com',
  },
  updateInput: {
    title: updatedTitle,
  },
};

try {
  cleanupResponses.beforeByRemote = await runGraphqlRequest(deleteByRemoteDocument, { remoteId });

  primaryResponse = await runGraphqlRequest(primaryDocument, primaryVariables);
  await assertGraphqlOk('primary', primaryResponse);
  assertNoUserErrors('create-external', primaryResponse.payload, 'createExternal');
  assertNoUserErrors('update-external', primaryResponse.payload, 'updateExternal');
  assertRoundTripFields(
    'createExternal.marketingActivity',
    readPath(primaryResponse.payload, ['data', 'createExternal', 'marketingActivity']),
  );
  assertActivityAppMatches(
    'createExternal.marketingActivity',
    readPath(primaryResponse.payload, ['data', 'createExternal', 'marketingActivity']),
    appContext,
  );
  assertRoundTripFields(
    'updateExternal.marketingActivity',
    readPath(primaryResponse.payload, ['data', 'updateExternal', 'marketingActivity']),
  );
  assertActivityAppMatches(
    'updateExternal.marketingActivity',
    readPath(primaryResponse.payload, ['data', 'updateExternal', 'marketingActivity']),
    appContext,
  );

  activityId = readStringPath(
    primaryResponse.payload,
    ['data', 'updateExternal', 'marketingActivity', 'id'],
    'activityId',
  );

  readAfterUpdateResponse = await runGraphqlRequest(readDocument, {
    activityId,
    remoteIds: [remoteId],
  });
  await assertGraphqlOk('read-after-update', readAfterUpdateResponse);
  assertRoundTripFields(
    'readAfterUpdate.marketingActivity',
    readPath(readAfterUpdateResponse.payload, ['data', 'marketingActivity']),
  );
  assertActivityAppMatches(
    'readAfterUpdate.marketingActivity',
    readPath(readAfterUpdateResponse.payload, ['data', 'marketingActivity']),
    appContext,
  );
  const connectionNodes = readPath(readAfterUpdateResponse.payload, ['data', 'marketingActivities', 'nodes']);
  if (!Array.isArray(connectionNodes) || !connectionNodes[0]) {
    throw new Error(
      `readAfterUpdate.marketingActivities.nodes[0] missing: ${JSON.stringify(readAfterUpdateResponse.payload)}`,
    );
  }
  assertActivityAppMatches('readAfterUpdate.marketingActivities.nodes[0]', connectionNodes[0], appContext);
  const readTitle = readStringPath(
    readAfterUpdateResponse.payload,
    ['data', 'marketingActivity', 'title'],
    'readAfterUpdate.title',
  );
  if (readTitle !== updatedTitle) {
    throw new Error(`read-after-update title expected ${updatedTitle}, got ${readTitle}`);
  }
} finally {
  cleanupResponses.afterByRemote = await runGraphqlRequest(deleteByRemoteDocument, { remoteId });
  if (activityId) {
    cleanupResponses.afterById = await runGraphqlRequest(deleteByIdDocument, { marketingActivityId: activityId });
  }
}

if (!primaryResponse || !readAfterUpdateResponse || !activityId) {
  throw new Error('capture did not complete every required operation');
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'marketing-activity-create-external-read-after-write',
      apiVersion,
      storeDomain,
      capturedAt: new Date().toISOString(),
      scopeEvidence: {
        app: appContext,
      },
      operations: {
        appContext: {
          query: appContextDocument,
          variables: {},
          response: appContextResponse,
        },
        primary: {
          query: primaryDocument,
          variables: primaryVariables,
          response: primaryResponse,
        },
        readAfterUpdate: {
          query: readDocument,
          variables: {
            activityId,
            remoteIds: [remoteId],
          },
          response: readAfterUpdateResponse,
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
      apiVersion,
      storeDomain,
      remoteId,
      activityId,
      cleanup: Object.keys(cleanupResponses),
    },
    null,
    2,
  ),
);
