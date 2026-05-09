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
const outputPath = path.join(outputDir, 'marketing-activity-status-label.json');
const primaryDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-status-label.graphql'),
  'utf8',
);
const readDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-status-label-read.graphql'),
  'utf8',
);

const deleteByRemoteDocument = `#graphql
  mutation MarketingActivityStatusLabelCleanup($remoteId: String) {
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

const pendingAdProbeDocument = `#graphql
  mutation MarketingActivityStatusLabelPendingAdProbe($input: MarketingActivityCreateExternalInput!) {
    marketingActivityCreateExternal(input: $input) {
      marketingActivity {
        id
        title
        status
        statusLabel
        tactic
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const inventoryDocument = `#graphql
  query MarketingActivityStatusLabelInventory($first: Int!, $query: String) {
    marketingActivities(first: $first, sortKey: CREATED_AT, reverse: true, query: $query) {
      nodes {
        id
        title
        status
        statusLabel
        tactic
        targetStatus
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

function readStringPath(payload: unknown, parts: string[], label: string): string {
  const value = readPath(payload, parts);
  if (typeof value !== 'string') {
    throw new Error(`${label} missing string at ${parts.join('.')}: ${JSON.stringify(payload)}`);
  }
  return value;
}

function readUserErrors(payload: unknown, parts: string[]): unknown[] {
  const value = readPath(payload, parts);
  return Array.isArray(value) ? value : [];
}

async function assertGraphqlOk(label: string, result: GraphqlResult): Promise<void> {
  if (result.status >= 200 && result.status < 300 && !readRecord(result.payload)?.['errors']) {
    return;
  }
  console.error(JSON.stringify(result.payload, null, 2));
  throw new Error(`${label} failed with HTTP ${result.status}`);
}

function assertNoUserErrors(label: string, payload: unknown, root: string): void {
  const userErrors = readUserErrors(payload, ['data', root, 'userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function externalInput({
  title,
  remoteId,
  status,
  tactic,
  marketingChannelType,
}: {
  title: string;
  remoteId: string;
  status: string;
  tactic: string;
  marketingChannelType: string;
}): Record<string, unknown> {
  const lowercaseTactic = tactic.toLowerCase();
  return {
    title,
    remoteId,
    status,
    remoteUrl: `https://example.com/${remoteId}`,
    tactic,
    marketingChannelType,
    urlParameterValue: `utm_campaign=${remoteId}`,
    utm: {
      campaign: remoteId,
      source: marketingChannelType.toLowerCase(),
      medium: lowercaseTactic,
    },
  };
}

async function cleanupRemoteIds(remoteIds: string[]): Promise<Record<string, unknown>> {
  const responses: Record<string, unknown> = {};
  for (const remoteId of remoteIds) {
    try {
      const result = await runGraphqlRequest(deleteByRemoteDocument, { remoteId });
      responses[remoteId] = result.payload;
    } catch (error) {
      responses[remoteId] = { error: (error as Error).message };
    }
  }
  return responses;
}

const suffix = randomSuffix();
const remoteIds = {
  activePost: `status-label-active-post-${suffix}`,
  activeNewsletter: `status-label-active-newsletter-${suffix}`,
  inactivePost: `status-label-inactive-post-${suffix}`,
  inactiveNewsletter: `status-label-inactive-newsletter-${suffix}`,
  inactiveAd: `status-label-inactive-ad-${suffix}`,
  deletedExternally: `status-label-deleted-externally-${suffix}`,
};
const cleanupRemoteIdList = Object.values(remoteIds);

const pendingAdProbeVariables = {
  input: externalInput({
    title: `Status label pending ad ${suffix}`,
    remoteId: `status-label-pending-ad-${suffix}`,
    status: 'PENDING',
    tactic: 'AD',
    marketingChannelType: 'SEARCH',
  }),
};

const primaryVariables = {
  activePostInput: externalInput({
    title: `Status label active post ${suffix}`,
    remoteId: remoteIds.activePost,
    status: 'ACTIVE',
    tactic: 'POST',
    marketingChannelType: 'SOCIAL',
  }),
  activeNewsletterInput: externalInput({
    title: `Status label active newsletter ${suffix}`,
    remoteId: remoteIds.activeNewsletter,
    status: 'ACTIVE',
    tactic: 'NEWSLETTER',
    marketingChannelType: 'EMAIL',
  }),
  inactivePostInput: externalInput({
    title: `Status label inactive post ${suffix}`,
    remoteId: remoteIds.inactivePost,
    status: 'INACTIVE',
    tactic: 'POST',
    marketingChannelType: 'SOCIAL',
  }),
  inactiveNewsletterInput: externalInput({
    title: `Status label inactive newsletter ${suffix}`,
    remoteId: remoteIds.inactiveNewsletter,
    status: 'INACTIVE',
    tactic: 'NEWSLETTER',
    marketingChannelType: 'EMAIL',
  }),
  inactiveAdInput: externalInput({
    title: `Status label inactive ad ${suffix}`,
    remoteId: remoteIds.inactiveAd,
    status: 'INACTIVE',
    tactic: 'AD',
    marketingChannelType: 'SEARCH',
  }),
  deletedSeedInput: externalInput({
    title: `Status label deleted externally ${suffix}`,
    remoteId: remoteIds.deletedExternally,
    status: 'ACTIVE',
    tactic: 'AD',
    marketingChannelType: 'SEARCH',
  }),
  deletedRemoteId: remoteIds.deletedExternally,
  deletedExternallyInput: {
    status: 'DELETED_EXTERNALLY',
  },
};

let pendingAdProbeResponse: GraphqlResult | null = null;
let pendingAdInventoryResponse: GraphqlResult | null = null;
let primaryResponse: GraphqlResult | null = null;
let readResponse: GraphqlResult | null = null;
let cleanupResponses: Record<string, unknown> = {};

try {
  pendingAdProbeResponse = await runGraphqlRequest(pendingAdProbeDocument, pendingAdProbeVariables);
  if (!readRecord(pendingAdProbeResponse.payload)?.['errors']) {
    throw new Error(
      'PENDING ad create unexpectedly succeeded; update this capture to record the success path instead of the blocker.',
    );
  }
  pendingAdInventoryResponse = await runGraphqlRequest(inventoryDocument, {
    first: 100,
    query: 'tactic:AD',
  });
  await assertGraphqlOk('pending-ad-inventory', pendingAdInventoryResponse);

  primaryResponse = await runGraphqlRequest(primaryDocument, primaryVariables);
  await assertGraphqlOk('primary', primaryResponse);

  for (const root of [
    'activePost',
    'activeNewsletter',
    'inactivePost',
    'inactiveNewsletter',
    'inactiveAd',
    'deletedSeed',
    'deletedExternally',
  ]) {
    assertNoUserErrors(root, primaryResponse.payload, root);
  }

  const readVariables = {
    activePostId: readStringPath(
      primaryResponse.payload,
      ['data', 'activePost', 'marketingActivity', 'id'],
      'activePostId',
    ),
    activeNewsletterId: readStringPath(
      primaryResponse.payload,
      ['data', 'activeNewsletter', 'marketingActivity', 'id'],
      'activeNewsletterId',
    ),
    inactivePostId: readStringPath(
      primaryResponse.payload,
      ['data', 'inactivePost', 'marketingActivity', 'id'],
      'inactivePostId',
    ),
    inactiveNewsletterId: readStringPath(
      primaryResponse.payload,
      ['data', 'inactiveNewsletter', 'marketingActivity', 'id'],
      'inactiveNewsletterId',
    ),
    inactiveAdId: readStringPath(
      primaryResponse.payload,
      ['data', 'inactiveAd', 'marketingActivity', 'id'],
      'inactiveAdId',
    ),
    deletedExternallyId: readStringPath(
      primaryResponse.payload,
      ['data', 'deletedExternally', 'marketingActivity', 'id'],
      'deletedExternallyId',
    ),
  };
  readResponse = await runGraphqlRequest(readDocument, readVariables);
  await assertGraphqlOk('read', readResponse);
} finally {
  cleanupResponses = await cleanupRemoteIds(cleanupRemoteIdList);
}

if (!pendingAdProbeResponse || !pendingAdInventoryResponse || !primaryResponse || !readResponse) {
  throw new Error('capture did not complete primary and read responses');
}

await mkdir(outputDir, { recursive: true });
const capture = {
  scenarioId: 'marketing-activity-status-label',
  apiVersion,
  storeDomain,
  capturedAt: new Date().toISOString(),
  setup: {
    disposableRemoteIds: remoteIds,
    cleanup: 'Each disposable external marketing activity is deleted by remoteId in the finally block.',
  },
  blockedBranches: {
    pendingAd: {
      reason:
        'The current conformance API rejects MarketingActivityCreateExternalInput.status = PENDING, and the disposable shop has no readable PENDING+AD marketing activity to use as existing evidence.',
      attemptedCreate: {
        request: {
          query: pendingAdProbeDocument,
          variables: pendingAdProbeVariables,
        },
        response: pendingAdProbeResponse,
      },
      inventoryProbe: {
        request: {
          query: inventoryDocument,
          variables: {
            first: 100,
            query: 'tactic:AD',
          },
        },
        response: pendingAdInventoryResponse,
      },
    },
  },
  operations: {
    primary: {
      request: {
        query: primaryDocument,
        variables: primaryVariables,
      },
      response: primaryResponse,
    },
    read: {
      request: {
        query: readDocument,
        variables: {
          activePostId: readStringPath(
            primaryResponse.payload,
            ['data', 'activePost', 'marketingActivity', 'id'],
            'activePostId',
          ),
          activeNewsletterId: readStringPath(
            primaryResponse.payload,
            ['data', 'activeNewsletter', 'marketingActivity', 'id'],
            'activeNewsletterId',
          ),
          inactivePostId: readStringPath(
            primaryResponse.payload,
            ['data', 'inactivePost', 'marketingActivity', 'id'],
            'inactivePostId',
          ),
          inactiveNewsletterId: readStringPath(
            primaryResponse.payload,
            ['data', 'inactiveNewsletter', 'marketingActivity', 'id'],
            'inactiveNewsletterId',
          ),
          inactiveAdId: readStringPath(
            primaryResponse.payload,
            ['data', 'inactiveAd', 'marketingActivity', 'id'],
            'inactiveAdId',
          ),
          deletedExternallyId: readStringPath(
            primaryResponse.payload,
            ['data', 'deletedExternally', 'marketingActivity', 'id'],
            'deletedExternallyId',
          ),
        },
      },
      response: readResponse,
    },
    cleanup: cleanupResponses,
  },
  expectedLabels: {
    activePost: readStringPath(
      primaryResponse.payload,
      ['data', 'activePost', 'marketingActivity', 'statusLabel'],
      'activePostLabel',
    ),
    activeNewsletter: readStringPath(
      primaryResponse.payload,
      ['data', 'activeNewsletter', 'marketingActivity', 'statusLabel'],
      'activeNewsletterLabel',
    ),
    inactivePost: readStringPath(
      primaryResponse.payload,
      ['data', 'inactivePost', 'marketingActivity', 'statusLabel'],
      'inactivePostLabel',
    ),
    inactiveNewsletter: readStringPath(
      primaryResponse.payload,
      ['data', 'inactiveNewsletter', 'marketingActivity', 'statusLabel'],
      'inactiveNewsletterLabel',
    ),
    inactiveAd: readStringPath(
      primaryResponse.payload,
      ['data', 'inactiveAd', 'marketingActivity', 'statusLabel'],
      'inactiveAdLabel',
    ),
    deletedExternally: readStringPath(
      primaryResponse.payload,
      ['data', 'deletedExternally', 'marketingActivity', 'statusLabel'],
      'deletedExternallyLabel',
    ),
  },
};

await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      apiVersion,
      storeDomain,
      labels: capture.expectedLabels,
      cleanupRemoteIds: cleanupRemoteIdList,
    },
    null,
    2,
  ),
);
