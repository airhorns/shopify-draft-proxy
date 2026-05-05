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
const outputPath = path.join(outputDir, 'marketing-activity-delete-external-guards.json');

const primaryDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-delete-external-guards.graphql'),
  'utf8',
);

const createDocument = `#graphql
  mutation CreateMarketingActivity($input: MarketingActivityCreateExternalInput!) {
    marketingActivityCreateExternal(input: $input) {
      marketingActivity {
        id
        title
        parentRemoteId
        hierarchyLevel
        marketingEvent {
          id
          remoteId
          channelHandle
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

const deleteDocument = `#graphql
  mutation DeleteMarketingActivity($remoteId: String) {
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

const inventoryDocument = `#graphql
  query NativeMarketingActivityInventory {
    marketingActivities(first: 100) {
      nodes {
        id
        title
        isExternal
        marketingEvent {
          id
          remoteId
        }
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

function hasTopLevelErrors(payload: unknown): boolean {
  return Array.isArray(readRecord(payload)?.['errors']);
}

async function assertGraphqlOk(label: string, result: GraphqlResult): Promise<void> {
  if (result.status >= 200 && result.status < 300 && !hasTopLevelErrors(result.payload)) {
    return;
  }
  console.error(JSON.stringify(result.payload, null, 2));
  throw new Error(`${label} failed with HTTP ${result.status}`);
}

function assertUserErrorCode(label: string, payload: unknown, root: string, expectedCode: string): void {
  const actual = readFirstUserErrorCode(payload, root);
  if (actual !== expectedCode) {
    throw new Error(`${label} expected ${expectedCode}, got ${actual ?? '<none>'}: ${JSON.stringify(payload)}`);
  }
}

function assertNoUserErrors(label: string, payload: unknown, root: string): void {
  const userErrors = readUserErrors(payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function inputFor(label: string, remoteId: string): Record<string, unknown> {
  return {
    title: `HAR-687 ${label}`,
    remoteId,
    status: 'ACTIVE',
    remoteUrl: `https://example.com/har-687/${label}`,
    tactic: 'NEWSLETTER',
    marketingChannelType: 'EMAIL',
    urlParameterValue: `har687-${label}`.replace(/[^a-zA-Z0-9_-]/gu, '-'),
    utm: {
      campaign: `har-687-${label}`,
      source: 'email',
      medium: 'newsletter',
    },
  };
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function waitForDeleteAllIdle(remoteId: string): Promise<unknown[]> {
  const attempts: unknown[] = [];
  for (let index = 0; index < 18; index += 1) {
    const input = inputFor(`preflight-${index}-${remoteId}`, `${remoteId}-${index}`);
    const result = await runGraphqlRequest(createDocument, { input });
    await assertGraphqlOk(`preflight-create-${index}`, result);
    attempts.push({ input, response: result });
    const code = readFirstUserErrorCode(result.payload, 'marketingActivityCreateExternal');
    if (code !== 'DELETE_JOB_ENQUEUED') {
      await runGraphqlRequest(deleteDocument, { remoteId: input.remoteId });
      return attempts;
    }
    await sleep(5000);
  }
  return attempts;
}

const suffix = randomSuffix();
const createAfterDeleteAllInput = inputFor(`after-delete-all-${suffix}`, `har-687-after-delete-all-${suffix}`);
const preflightAttempts = await waitForDeleteAllIdle(`har-687-preflight-${suffix}`);

const nativeInventory = await runGraphqlRequest(inventoryDocument, {});
await assertGraphqlOk('native-inventory', nativeInventory);

const parentAttempts: Array<{ name: string; variables: Record<string, unknown>; response: GraphqlResult }> = [];
for (const attempt of [
  {
    name: 'campaign-no-channel',
    input: {
      ...inputFor(`parent-${suffix}`, `har-687-parent-${suffix}`),
      hierarchyLevel: 'CAMPAIGN',
    },
  },
  {
    name: 'campaign-with-email',
    input: {
      ...inputFor(`parent-email-${suffix}`, `har-687-parent-email-${suffix}`),
      hierarchyLevel: 'CAMPAIGN',
      channelHandle: 'email',
    },
  },
  {
    name: 'campaign-with-app',
    input: {
      ...inputFor(`parent-app-${suffix}`, `har-687-parent-app-${suffix}`),
      hierarchyLevel: 'CAMPAIGN',
      channelHandle: process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] || 'hermes-conformance-products',
    },
  },
]) {
  const response = await runGraphqlRequest(createDocument, { input: attempt.input });
  await assertGraphqlOk(attempt.name, response);
  parentAttempts.push({ name: attempt.name, variables: { input: attempt.input }, response });
}

const primaryVariables = { createAfterDeleteAllInput };
const primary = await runGraphqlRequest(primaryDocument, primaryVariables);
await assertGraphqlOk('primary-delete-external-guards', primary);
assertUserErrorCode('no-args-delete', primary.payload, 'noArgsDelete', 'INVALID_DELETE_ACTIVITY_EXTERNAL_ARGUMENTS');
assertNoUserErrors('delete-all-external', primary.payload, 'deleteAllExternal');
assertUserErrorCode('create-after-delete-all', primary.payload, 'createAfterDeleteAll', 'DELETE_JOB_ENQUEUED');

const cleanup = {
  createAfterDeleteAll: await runGraphqlRequest(deleteDocument, {
    remoteId: createAfterDeleteAllInput.remoteId,
  }),
};

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'marketing-activity-delete-external-guards',
      apiVersion,
      storeDomain,
      capturedAt: new Date().toISOString(),
      operations: {
        primary: {
          request: {
            query: primaryDocument,
            variables: primaryVariables,
          },
          response: primary,
        },
      },
      blockerProbes: {
        parentChildSetup: {
          summary:
            'The disposable shop did not expose a recognized channelHandle, so creating a campaign-level parent external activity for child-event delete validation was blocked.',
          attempts: parentAttempts,
        },
        nativeDeleteSetup: {
          summary:
            'No non-external marketing activities were discoverable through Admin GraphQL in the disposable shop, and native success creation remains blocked by the missing deprecated MarketingActivityExtension recorded in marketing-native-activity-lifecycle.',
          inventory: nativeInventory,
        },
      },
      preflight: {
        deleteAllIdleAttempts: preflightAttempts,
      },
      cleanup,
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
      parentChildSetup: 'blocked-no-recognized-channel-handle',
      nativeDeleteSetup: 'blocked-no-non-external-activity',
    },
    null,
    2,
  ),
);
