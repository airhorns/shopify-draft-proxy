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
const outputPath = path.join(outputDir, 'marketing-activity-upsert-external-validation.json');

const primaryDocument = await readFile(
  path.join('config', 'parity-requests', 'marketing', 'marketing-activity-upsert-external-validation.graphql'),
  'utf8',
);

const deleteDocument = `#graphql
  mutation MarketingActivityUpsertExternalValidationCleanup($remoteId: String) {
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

function readMarketingActivity(payload: unknown, root: string): unknown {
  return readPath(payload, ['data', root, 'marketingActivity']);
}

function readFirstUserError(payload: unknown, root: string): Record<string, unknown> | null {
  const [first] = readUserErrors(payload, root);
  return readRecord(first);
}

function readFirstUserErrorMessage(payload: unknown, root: string): string | null {
  const message = readFirstUserError(payload, root)?.['message'];
  return typeof message === 'string' ? message : null;
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

function assertNoUserErrors(label: string, payload: unknown, root: string): void {
  const userErrors = readUserErrors(payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function assertNullMarketingActivity(label: string, payload: unknown, root: string): void {
  const marketingActivity = readMarketingActivity(payload, root);
  if (marketingActivity !== null) {
    throw new Error(`${label} expected null marketingActivity, got ${JSON.stringify(marketingActivity)}`);
  }
}

function assertUserErrorMessageWithoutCode(
  label: string,
  payload: unknown,
  root: string,
  expectedMessage: string,
): void {
  const actualMessage = readFirstUserErrorMessage(payload, root);
  const actualCode = readFirstUserError(payload, root)?.['code'] ?? null;
  if (actualMessage !== expectedMessage || actualCode !== null) {
    throw new Error(
      `${label} expected message ${JSON.stringify(expectedMessage)} with null code, got ${JSON.stringify(
        readFirstUserError(payload, root),
      )}`,
    );
  }
  assertNullMarketingActivity(label, payload, root);
}

function baseInput(label: string, suffix: string, remoteId: string): Record<string, unknown> {
  return {
    title: `Marketing upsert validation ${label} ${suffix}`,
    remoteId,
    status: 'ACTIVE',
    remoteUrl: `https://example.com/marketing-upsert-validation/${label}/${suffix}`,
    tactic: 'NEWSLETTER',
    marketingChannelType: 'EMAIL',
    utm: {
      campaign: `marketing-upsert-validation-${label}-${suffix}`,
      source: 'email',
      medium: 'newsletter',
    },
  };
}

const suffix = randomSuffix();
const utmSeedRemoteId = `marketing-upsert-validation-utm-seed-${suffix}`;
const urlSeedRemoteId = `marketing-upsert-validation-url-seed-${suffix}`;
const duplicateUrlParameterValue = `marketing-upsert-validation-url-${suffix}`;

const utmSeedInput = {
  ...baseInput('utm-seed', suffix, utmSeedRemoteId),
  urlParameterValue: `marketing-upsert-validation-utm-seed-${suffix}`,
};
const urlSeedInput = {
  ...baseInput('url-seed', suffix, urlSeedRemoteId),
  urlParameterValue: duplicateUrlParameterValue,
};

const primaryVariables = {
  currencyMismatchInput: {
    ...baseInput('currency-mismatch', suffix, `marketing-upsert-validation-currency-${suffix}`),
    budget: {
      budgetType: 'DAILY',
      total: {
        amount: '1.00',
        currencyCode: 'USD',
      },
    },
    adSpend: {
      amount: '1.00',
      currencyCode: 'EUR',
    },
  },
  utmSeedInput,
  duplicateUtmCampaignInput: {
    ...baseInput('duplicate-utm', suffix, `marketing-upsert-validation-duplicate-utm-${suffix}`),
    urlParameterValue: `marketing-upsert-validation-duplicate-utm-${suffix}`,
    utm: utmSeedInput.utm,
  },
  urlSeedInput,
  duplicateUrlParameterValueInput: {
    ...baseInput('duplicate-url', suffix, `marketing-upsert-validation-duplicate-url-${suffix}`),
    urlParameterValue: duplicateUrlParameterValue,
  },
};

const primary = await runGraphqlRequest(primaryDocument, primaryVariables);
await assertGraphqlOk('primary-validation', primary);

const cleanupRemoteIds = [
  utmSeedRemoteId,
  urlSeedRemoteId,
  `marketing-upsert-validation-currency-${suffix}`,
  `marketing-upsert-validation-duplicate-utm-${suffix}`,
  `marketing-upsert-validation-duplicate-url-${suffix}`,
];
const cleanupResponses: Record<string, GraphqlResult> = {};
for (const remoteId of cleanupRemoteIds) {
  cleanupResponses[remoteId] = await runGraphqlRequest(deleteDocument, { remoteId });
}

assertUserErrorMessageWithoutCode(
  'currency-mismatch',
  primary.payload,
  'currencyMismatch',
  'Currency code is not matching between budget and ad spend',
);
assertNoUserErrors('utm-seed', primary.payload, 'utmSeed');
assertUserErrorMessageWithoutCode(
  'duplicate-utm-campaign',
  primary.payload,
  'duplicateUtmCampaign',
  'Validation failed: Utm campaign has already been taken',
);
assertNoUserErrors('url-seed', primary.payload, 'urlSeed');
assertUserErrorMessageWithoutCode(
  'duplicate-url-parameter-value',
  primary.payload,
  'duplicateUrlParameterValue',
  'Validation failed: Url parameter value has already been taken, Url parameter value has already been taken',
);

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'marketing-activity-upsert-external-validation',
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
      roots: Object.keys(readRecord(readPath(primary.payload, ['data'])) ?? {}),
    },
    null,
    2,
  ),
);
