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
const outputPath = path.join(outputDir, 'marketing-activity-create-external-default-status.json');
const primaryDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-create-external-default-status.graphql'),
  'utf8',
);
const readDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-create-external-default-status-read.graphql'),
  'utf8',
);

const deleteByRemoteDocument = `#graphql
  mutation MarketingActivityCreateExternalDefaultStatusCleanup($remoteId: String) {
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

const upsertNoStatusProbeDocument = `#graphql
  mutation MarketingActivityCreateExternalDefaultStatusUpsertProbe($input: MarketingActivityUpsertExternalInput!) {
    upsertNoStatus: marketingActivityUpsertExternal(input: $input) {
      marketingActivity {
        id
        title
        status
        statusLabel
      }
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
}: {
  title: string;
  remoteId: string;
  status?: string;
}): Record<string, unknown> {
  return {
    title,
    remoteId,
    ...(status ? { status } : {}),
    remoteUrl: `https://example.com/${remoteId}`,
    tactic: 'NEWSLETTER',
    marketingChannelType: 'EMAIL',
    urlParameterValue: `utm_campaign=${remoteId}`,
    utm: {
      campaign: remoteId,
      source: 'email',
      medium: 'newsletter',
    },
  };
}

function assertActivityStatus(
  label: string,
  payload: unknown,
  parts: string[],
  expectedStatus: string,
  expectedStatusLabel: string,
): void {
  const status = readPath(payload, [...parts, 'status']);
  const statusLabel = readPath(payload, [...parts, 'statusLabel']);
  if (status !== expectedStatus || statusLabel !== expectedStatusLabel) {
    throw new Error(
      `${label} expected ${expectedStatus}/${expectedStatusLabel}, got ${JSON.stringify({
        status,
        statusLabel,
      })}`,
    );
  }
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
  createNoStatus: `default-status-create-${suffix}`,
  createActive: `default-status-active-${suffix}`,
  upsertNoStatusProbe: `default-status-upsert-probe-${suffix}`,
};
const cleanupRemoteIdList = Object.values(remoteIds);

const primaryVariables = {
  createNoStatusInput: externalInput({
    title: `Default status create ${suffix}`,
    remoteId: remoteIds.createNoStatus,
  }),
  createActiveInput: externalInput({
    title: `Default status active control ${suffix}`,
    remoteId: remoteIds.createActive,
    status: 'ACTIVE',
  }),
};
const upsertNoStatusProbeVariables = {
  input: externalInput({
    title: `Default status upsert probe ${suffix}`,
    remoteId: remoteIds.upsertNoStatusProbe,
  }),
};

let primaryResponse: GraphqlResult | null = null;
let readResponse: GraphqlResult | null = null;
let upsertNoStatusProbeResponse: GraphqlResult | null = null;
let cleanupResponses: Record<string, unknown> = {};

try {
  upsertNoStatusProbeResponse = await runGraphqlRequest(upsertNoStatusProbeDocument, upsertNoStatusProbeVariables);
  primaryResponse = await runGraphqlRequest(primaryDocument, primaryVariables);
  await assertGraphqlOk('primary', primaryResponse);

  for (const root of ['createNoStatus', 'createActive']) {
    assertNoUserErrors(root, primaryResponse.payload, root);
  }
  assertActivityStatus(
    'create-no-status',
    primaryResponse.payload,
    ['data', 'createNoStatus', 'marketingActivity'],
    'UNDEFINED',
    'Undefined',
  );
  assertActivityStatus(
    'create-active',
    primaryResponse.payload,
    ['data', 'createActive', 'marketingActivity'],
    'ACTIVE',
    'Sending',
  );

  const readVariables = {
    createNoStatusId: readStringPath(
      primaryResponse.payload,
      ['data', 'createNoStatus', 'marketingActivity', 'id'],
      'createNoStatusId',
    ),
    createActiveId: readStringPath(
      primaryResponse.payload,
      ['data', 'createActive', 'marketingActivity', 'id'],
      'createActiveId',
    ),
  };
  readResponse = await runGraphqlRequest(readDocument, readVariables);
  await assertGraphqlOk('read', readResponse);
  assertActivityStatus(
    'read-create-no-status',
    readResponse.payload,
    ['data', 'createNoStatus'],
    'UNDEFINED',
    'Undefined',
  );
  assertActivityStatus('read-create-active', readResponse.payload, ['data', 'createActive'], 'ACTIVE', 'Sending');
} finally {
  cleanupResponses = await cleanupRemoteIds(cleanupRemoteIdList);
}

if (!primaryResponse || !readResponse || !upsertNoStatusProbeResponse) {
  throw new Error('capture did not complete primary and read responses');
}

await mkdir(outputDir, { recursive: true });
const capture = {
  scenarioId: 'marketing-activity-create-external-default-status',
  apiVersion,
  storeDomain,
  capturedAt: new Date().toISOString(),
  setup: {
    disposableRemoteIds: remoteIds,
    cleanup: 'Each disposable external marketing activity is deleted by remoteId in the finally block.',
  },
  operations: {
    upsertNoStatusProbe: {
      request: {
        query: upsertNoStatusProbeDocument,
        variables: upsertNoStatusProbeVariables,
      },
      response: upsertNoStatusProbeResponse,
    },
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
          createNoStatusId: readStringPath(
            primaryResponse.payload,
            ['data', 'createNoStatus', 'marketingActivity', 'id'],
            'createNoStatusId',
          ),
          createActiveId: readStringPath(
            primaryResponse.payload,
            ['data', 'createActive', 'marketingActivity', 'id'],
            'createActiveId',
          ),
        },
      },
      response: readResponse,
    },
    cleanup: cleanupResponses,
  },
  expectedStatuses: {
    createNoStatus: {
      status: readStringPath(
        primaryResponse.payload,
        ['data', 'createNoStatus', 'marketingActivity', 'status'],
        'createNoStatusStatus',
      ),
      statusLabel: readStringPath(
        primaryResponse.payload,
        ['data', 'createNoStatus', 'marketingActivity', 'statusLabel'],
        'createNoStatusStatusLabel',
      ),
    },
    createActive: {
      status: readStringPath(
        primaryResponse.payload,
        ['data', 'createActive', 'marketingActivity', 'status'],
        'createActiveStatus',
      ),
      statusLabel: readStringPath(
        primaryResponse.payload,
        ['data', 'createActive', 'marketingActivity', 'statusLabel'],
        'createActiveStatusLabel',
      ),
    },
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
      expectedStatuses: capture.expectedStatuses,
      cleanupRemoteIds: cleanupRemoteIdList,
    },
    null,
    2,
  ),
);
