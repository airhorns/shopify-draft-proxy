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
const outputPath = path.join(outputDir, 'marketing-activity-upsert-immutable-fields.json');

const createDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-immutable-create.graphql'),
  'utf8',
);
const upsertDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-immutable-upsert.graphql'),
  'utf8',
);
const updateDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-immutable-update.graphql'),
  'utf8',
);

const deleteDocument = `#graphql
  mutation MarketingActivityImmutableCleanup($remoteId: String) {
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

async function runCase(name: string, query: string, variables: Record<string, unknown>) {
  const response = await runGraphqlRequest(query, variables);
  await assertGraphqlOk(name, response);
  return { name, variables, response };
}

const suffix = randomSuffix();
const childRemoteId = `har-681-child-${suffix}`;
const channelHandle = `har-681-channel-${suffix}`;
const childUrlParameterValue = `har681-child-${suffix}`;
const childUtm = { campaign: `har-681-campaign-child-${suffix}`, source: 'email', medium: 'newsletter' };

function baseInput(kind: string) {
  return {
    status: 'ACTIVE',
    remoteUrl: `https://example.com/har-681-${kind}`,
    tactic: 'NEWSLETTER',
    marketingChannelType: 'EMAIL',
    urlParameterValue: `har681-${kind}-${suffix}`,
    utm: { campaign: `har-681-campaign-${kind}-${suffix}`, source: 'email', medium: 'newsletter' },
  };
}

const childInput = {
  ...baseInput('child'),
  urlParameterValue: childUrlParameterValue,
  utm: childUtm,
  title: `HAR-681 Child ${suffix}`,
  remoteId: childRemoteId,
};

const cases: Array<{ name: string; variables: Record<string, unknown>; response: GraphqlResult }> = [];
const cleanupResponses: Record<string, unknown> = {};

try {
  const childCase = await runCase('child-create', createDocument, { input: childInput });
  assertNoUserErrors('child-create', childCase.response.payload, 'marketingActivityCreateExternal');
  cases.push(childCase);

  const updateCase = await runCase('update-success-by-remote-id', updateDocument, {
    remoteId: childRemoteId,
    utm: childUtm,
    input: {
      title: `HAR-681 Child Updated ${suffix}`,
      status: 'PAUSED',
      remoteUrl: 'https://example.com/har-681-updated',
    },
  });
  assertNoUserErrors('update-success-by-remote-id', updateCase.response.payload, 'marketingActivityUpdateExternal');
  cases.push(updateCase);

  const channelCase = await runCase('upsert-immutable-channel-handle', upsertDocument, {
    input: {
      ...childInput,
      title: `HAR-681 Channel Changed ${suffix}`,
      channelHandle: `${channelHandle}-changed`,
    },
  });
  assertUserErrorCode(
    'upsert-immutable-channel-handle',
    channelCase.response.payload,
    'marketingActivityUpsertExternal',
    'IMMUTABLE_CHANNEL_HANDLE',
  );
  cases.push(channelCase);

  const urlCase = await runCase('upsert-immutable-url-parameter', upsertDocument, {
    input: {
      ...childInput,
      title: `HAR-681 URL Changed ${suffix}`,
      urlParameterValue: `${childUrlParameterValue}-changed`,
    },
  });
  assertUserErrorCode(
    'upsert-immutable-url-parameter',
    urlCase.response.payload,
    'marketingActivityUpsertExternal',
    'IMMUTABLE_URL_PARAMETER',
  );
  cases.push(urlCase);

  const utmCase = await runCase('upsert-immutable-utm', upsertDocument, {
    input: {
      ...childInput,
      title: `HAR-681 UTM Changed ${suffix}`,
      utm: { ...childUtm, campaign: `${childUtm.campaign}-changed` },
    },
  });
  assertUserErrorCode(
    'upsert-immutable-utm',
    utmCase.response.payload,
    'marketingActivityUpsertExternal',
    'IMMUTABLE_UTM_PARAMETERS',
  );
  cases.push(utmCase);

  const invalidParentCase = await runCase('upsert-invalid-parent-remote-id', upsertDocument, {
    input: {
      ...childInput,
      title: `HAR-681 Missing Parent ${suffix}`,
      parentRemoteId: `har-681-missing-parent-${suffix}`,
    },
  });
  assertUserErrorCode(
    'upsert-invalid-parent-remote-id',
    invalidParentCase.response.payload,
    'marketingActivityUpsertExternal',
    'INVALID_REMOTE_ID',
  );
  cases.push(invalidParentCase);

  const hierarchyCase = await runCase('upsert-immutable-hierarchy-level', upsertDocument, {
    input: {
      ...childInput,
      title: `HAR-681 Hierarchy Changed ${suffix}`,
      hierarchyLevel: 'AD_GROUP',
    },
  });
  assertUserErrorCode(
    'upsert-immutable-hierarchy-level',
    hierarchyCase.response.payload,
    'marketingActivityUpsertExternal',
    'IMMUTABLE_HIERARCHY_LEVEL',
  );
  cases.push(hierarchyCase);
} finally {
  for (const remoteId of [childRemoteId]) {
    cleanupResponses[remoteId] = await runGraphqlRequest(deleteDocument, { remoteId });
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'marketing-activity-upsert-immutable-fields',
      apiVersion,
      storeDomain,
      capturedAt: new Date().toISOString(),
      setup: {
        remoteIds: {
          child: childRemoteId,
        },
      },
      cases,
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
      cases: cases.map((entry) => entry.name),
      cleanupRemoteIds: Object.keys(cleanupResponses),
    },
    null,
    2,
  ),
);
