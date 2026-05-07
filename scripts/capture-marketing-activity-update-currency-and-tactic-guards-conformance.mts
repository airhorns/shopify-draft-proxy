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
const outputPath = path.join(outputDir, 'marketing-activity-update-currency-and-tactic-guards.json');

const primaryDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-update-currency-and-tactic-guards.graphql'),
  'utf8',
);
const updateFromStorefrontDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-update-from-storefront-guard.graphql'),
  'utf8',
);
const readDocument = await readFile(
  path.join(
    'config',
    'parity-requests',
    'marketing',
    'marketing-activity-update-currency-and-tactic-guards-read.graphql',
  ),
  'utf8',
);

const deleteByRemoteDocument = `#graphql
  mutation MarketingActivityGuardCleanupByRemote($remoteId: String) {
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
  mutation MarketingActivityGuardCleanupById($marketingActivityId: ID) {
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

function readFirstUserError(payload: unknown, root: string): Record<string, unknown> | null {
  const [first] = readUserErrors(payload, root);
  return readRecord(first);
}

function readStringPath(payload: unknown, parts: string[], label: string): string {
  const value = readPath(payload, parts);
  if (typeof value !== 'string') {
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

function assertNoUserErrors(label: string, payload: unknown, root: string): void {
  const userErrors = readUserErrors(payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function assertUserError(
  label: string,
  payload: unknown,
  root: string,
  expected: { field: string[] | null; message: string; code: string | null },
): void {
  const actual = readFirstUserError(payload, root);
  if (!actual) {
    throw new Error(`${label} returned no userErrors: ${JSON.stringify(payload)}`);
  }
  const field = actual['field'];
  const message = actual['message'];
  const code = actual['code'] ?? null;
  const fieldMatches =
    expected.field === null ? field === null : JSON.stringify(field) === JSON.stringify(expected.field);
  if (!fieldMatches || message !== expected.message || code !== expected.code) {
    throw new Error(`${label} returned unexpected userError: ${JSON.stringify(actual)}`);
  }
}

const suffix = randomSuffix();
const baselineRemoteId = `marketing-guard-baseline-${suffix}`;
const storefrontRemoteId = `marketing-guard-storefront-${suffix}`;
const baselineTitle = `Marketing guard baseline ${suffix}`;
const happyTitle = `Marketing guard happy ${suffix}`;
const storefrontTitle = `Marketing guard storefront ${suffix}`;
const baselineUtm = { campaign: `${baselineRemoteId}-campaign`, source: 'email', medium: 'newsletter' };

const primaryVariables = {
  baselineRemoteId,
  baselineInput: {
    title: baselineTitle,
    remoteId: baselineRemoteId,
    status: 'ACTIVE',
    remoteUrl: `https://example.com/${baselineRemoteId}`,
    tactic: 'NEWSLETTER',
    marketingChannelType: 'EMAIL',
    budget: { budgetType: 'DAILY', total: { amount: '10.00', currencyCode: 'USD' } },
    utm: baselineUtm,
  },
  happyUpdateInput: {
    title: happyTitle,
  },
  storefrontInput: {
    title: storefrontTitle,
    remoteId: storefrontRemoteId,
    status: 'ACTIVE',
    remoteUrl: `https://example.com/${storefrontRemoteId}`,
    tactic: 'STOREFRONT_APP',
    marketingChannelType: 'EMAIL',
    utm: { campaign: `${storefrontRemoteId}-campaign`, source: 'email', medium: 'newsletter' },
  },
  currencyMismatchInput: {
    title: `Marketing guard currency should not stage ${suffix}`,
    budget: { budgetType: 'DAILY', total: { amount: '1.00', currencyCode: 'USD' } },
    adSpend: { amount: '1.00', currencyCode: 'EUR' },
  },
  toStorefrontUpdateInput: {
    title: `Marketing guard update should not stage ${suffix}`,
    tactic: 'STOREFRONT_APP',
  },
  toStorefrontUpsertInput: {
    title: `Marketing guard upsert should not stage ${suffix}`,
    remoteId: baselineRemoteId,
    status: 'ACTIVE',
    remoteUrl: `https://example.com/${baselineRemoteId}-upsert`,
    tactic: 'STOREFRONT_APP',
    marketingChannelType: 'EMAIL',
    utm: baselineUtm,
  },
};

const updateToStorefrontError = {
  field: ['input'],
  message:
    'You can not update an activity tactic to STOREFRONT_APP. This type of tactic can only be specified when creating a new activity.',
  code: 'CANNOT_UPDATE_TACTIC_TO_STOREFRONT_APP',
};
const fromStorefrontError = {
  field: ['input'],
  message: 'You can not update an activity tactic from STOREFRONT_APP.',
  code: 'CANNOT_UPDATE_TACTIC_IF_ORIGINALLY_STOREFRONT_APP',
};

const cleanupResponses: Record<string, unknown> = {};
let baselineActivityId: string | null = null;
let storefrontActivityId: string | null = null;
let primaryResponse: GraphqlResult | null = null;
let readBaselineResponse: GraphqlResult | null = null;
let updateFromStorefrontResponse: GraphqlResult | null = null;
let readStorefrontResponse: GraphqlResult | null = null;

try {
  primaryResponse = await runGraphqlRequest(primaryDocument, primaryVariables);
  await assertGraphqlOk('primary', primaryResponse);
  assertNoUserErrors('baseline-create', primaryResponse.payload, 'baselineCreate');
  assertNoUserErrors('happy-update', primaryResponse.payload, 'happyUpdate');
  assertNoUserErrors('storefront-create', primaryResponse.payload, 'storefrontCreate');
  assertUserError('currency-mismatch-update', primaryResponse.payload, 'currencyMismatchUpdate', {
    field: ['input'],
    message: 'Currency code is not matching between budget and ad spend',
    code: null,
  });
  assertUserError('update-to-storefront', primaryResponse.payload, 'updateToStorefront', updateToStorefrontError);
  assertUserError('upsert-to-storefront', primaryResponse.payload, 'upsertToStorefront', updateToStorefrontError);

  baselineActivityId = readStringPath(
    primaryResponse.payload,
    ['data', 'baselineCreate', 'marketingActivity', 'id'],
    'baselineActivityId',
  );
  storefrontActivityId = readStringPath(
    primaryResponse.payload,
    ['data', 'storefrontCreate', 'marketingActivity', 'id'],
    'storefrontActivityId',
  );

  readBaselineResponse = await runGraphqlRequest(readDocument, { activityId: baselineActivityId });
  await assertGraphqlOk('read-baseline', readBaselineResponse);
  const readBaselineTitle = readStringPath(
    readBaselineResponse.payload,
    ['data', 'marketingActivity', 'title'],
    'readBaselineTitle',
  );
  if (readBaselineTitle !== happyTitle) {
    throw new Error(`expected baseline title ${happyTitle}, got ${readBaselineTitle}`);
  }

  updateFromStorefrontResponse = await runGraphqlRequest(updateFromStorefrontDocument, {
    marketingActivityId: storefrontActivityId,
    input: {
      title: `Marketing guard storefront should not stage ${suffix}`,
      tactic: 'NEWSLETTER',
    },
  });
  await assertGraphqlOk('update-from-storefront', updateFromStorefrontResponse);
  assertUserError(
    'update-from-storefront',
    updateFromStorefrontResponse.payload,
    'marketingActivityUpdateExternal',
    fromStorefrontError,
  );

  readStorefrontResponse = await runGraphqlRequest(readDocument, { activityId: storefrontActivityId });
  await assertGraphqlOk('read-storefront', readStorefrontResponse);
  const readStorefrontTitle = readStringPath(
    readStorefrontResponse.payload,
    ['data', 'marketingActivity', 'title'],
    'readStorefrontTitle',
  );
  const readStorefrontTactic = readStringPath(
    readStorefrontResponse.payload,
    ['data', 'marketingActivity', 'tactic'],
    'readStorefrontTactic',
  );
  if (readStorefrontTitle !== storefrontTitle || readStorefrontTactic !== 'STOREFRONT_APP') {
    throw new Error(
      `expected storefront activity to remain unchanged: ${JSON.stringify(readStorefrontResponse.payload)}`,
    );
  }
} finally {
  cleanupResponses.baselineByRemote = await runGraphqlRequest(deleteByRemoteDocument, { remoteId: baselineRemoteId });
  if (baselineActivityId) {
    cleanupResponses.baselineById = await runGraphqlRequest(deleteByIdDocument, {
      marketingActivityId: baselineActivityId,
    });
  }
  cleanupResponses.storefrontByRemote = await runGraphqlRequest(deleteByRemoteDocument, {
    remoteId: storefrontRemoteId,
  });
  if (storefrontActivityId) {
    cleanupResponses.storefrontById = await runGraphqlRequest(deleteByIdDocument, {
      marketingActivityId: storefrontActivityId,
    });
  }
}

if (!primaryResponse || !readBaselineResponse || !updateFromStorefrontResponse || !readStorefrontResponse) {
  throw new Error('capture did not complete every required operation');
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'marketing-activity-update-currency-and-tactic-guards',
      apiVersion,
      storeDomain,
      capturedAt: new Date().toISOString(),
      operations: {
        primary: {
          query: primaryDocument,
          variables: primaryVariables,
          response: primaryResponse,
        },
        readBaseline: {
          query: readDocument,
          variables: { activityId: baselineActivityId },
          response: readBaselineResponse,
        },
        updateFromStorefront: {
          query: updateFromStorefrontDocument,
          variables: {
            marketingActivityId: storefrontActivityId,
            input: {
              title: `Marketing guard storefront should not stage ${suffix}`,
              tactic: 'NEWSLETTER',
            },
          },
          response: updateFromStorefrontResponse,
        },
        readStorefront: {
          query: readDocument,
          variables: { activityId: storefrontActivityId },
          response: readStorefrontResponse,
        },
      },
      cleanup: cleanupResponses,
      notes:
        'The live shop allows creating an external STOREFRONT_APP activity and rejects updateExternal by marketingActivityId when changing it to NEWSLETTER. A same-remoteId upsert after STOREFRONT_APP creation is not used as live evidence because Shopify does not resolve that activity through the upsert remoteId update path on this conformance shop.',
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
      operations: ['primary', 'readBaseline', 'updateFromStorefront', 'readStorefront'],
      cleanup: Object.keys(cleanupResponses),
    },
    null,
    2,
  ),
);
