/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlResult = { status: number; payload: unknown };
type Variables = Record<string, { remoteId?: string }>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'marketing');
const outputPath = path.join(outputDir, 'marketing-activity-source-and-medium.json');

const documentPath = path.join(
  'config',
  'parity-requests',
  'marketing',
  'marketing-activity-source-and-medium.graphql',
);
const variablesPath = path.join(
  'config',
  'parity-requests',
  'marketing',
  'marketing-activity-source-and-medium.variables.json',
);
const primaryDocument = await readFile(documentPath, 'utf8');
const variables = JSON.parse(await readFile(variablesPath, 'utf8')) as Variables;

const deleteDocument = `#graphql
  mutation MarketingActivitySourceAndMediumCleanup($remoteId: String) {
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

const expectedSourceAndMedium: Record<string, string> = {
  abandonedCart: 'Abandoned cart email',
  affiliate: 'Affiliate link',
  loyalty: 'Loyalty program',
  retargetingFacebook: 'Facebook retargeting ad',
  retargetingNoDomain: 'Retargeting ad',
  messageFacebook: 'Message via Facebook Messenger',
  messageOther: 'Twitter message',
  adReferringDomain: 'Instagram ad',
  adNoDomain: 'Search ad',
};

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

function remoteIdsFromVariables(input: Variables): string[] {
  return Object.values(input)
    .map((value) => value.remoteId)
    .filter((value): value is string => typeof value === 'string' && value.length > 0);
}

async function cleanupRemoteIds(remoteIds: string[]): Promise<Record<string, unknown>> {
  const responses: Record<string, unknown> = {};
  for (const remoteId of remoteIds) {
    responses[remoteId] = await runGraphqlRequest(deleteDocument, { remoteId });
  }
  return responses;
}

const remoteIds = remoteIdsFromVariables(variables);
const cleanupBefore = await cleanupRemoteIds(remoteIds);
const primaryResponse = await runGraphqlRequest(primaryDocument, variables);
const cleanupAfter = await cleanupRemoteIds(remoteIds);

await assertGraphqlOk('primary', primaryResponse);

const capturedSourceAndMedium: Record<string, string> = {};
for (const [root, expected] of Object.entries(expectedSourceAndMedium)) {
  assertNoUserErrors(root, primaryResponse.payload, root);

  const activityValue = readStringPath(
    primaryResponse.payload,
    ['data', root, 'marketingActivity', 'sourceAndMedium'],
    `${root}.marketingActivity.sourceAndMedium`,
  );
  const eventValue = readStringPath(
    primaryResponse.payload,
    ['data', root, 'marketingActivity', 'marketingEvent', 'sourceAndMedium'],
    `${root}.marketingActivity.marketingEvent.sourceAndMedium`,
  );

  if (activityValue !== expected || eventValue !== expected) {
    throw new Error(
      `${root} expected ${expected}, got activity=${activityValue} event=${eventValue}: ${JSON.stringify(
        primaryResponse.payload,
      )}`,
    );
  }

  capturedSourceAndMedium[root] = activityValue;
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'marketing-activity-source-and-medium',
      apiVersion,
      storeDomain,
      capturedAt: new Date().toISOString(),
      operations: {
        createSourceAndMedium: {
          query: primaryDocument,
          variables,
          response: primaryResponse,
        },
      },
      capturedSourceAndMedium,
      cleanup: {
        before: cleanupBefore,
        after: cleanupAfter,
      },
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
      cases: Object.keys(expectedSourceAndMedium),
      cleanupRemoteIds: remoteIds,
    },
    null,
    2,
  ),
);
