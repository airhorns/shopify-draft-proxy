/* oxlint-disable no-console -- CLI capture scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { readFile, mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type Scenario = {
  label: string;
  query: string;
  variables: JsonRecord;
  status: number;
  response: unknown;
};

const requestDir = path.join('config', 'parity-requests', 'apps');
const readCurrentInstallationQuery = await readFile(
  path.join(requestDir, 'currentAppInstallation-observed-identity-read.graphql'),
  'utf8',
);
const createDelegateTokenMutation = await readFile(
  path.join(requestDir, 'delegateAccessTokenCreate-shop-payload.graphql'),
  'utf8',
);

const destroyDelegateTokenMutation = await readFile(
  path.join(requestDir, 'delegateAccessTokenDestroy-shop-payload.graphql'),
  'utf8',
);

const shopIdentityHydrateQuery = `#graphql
  query ProductPayloadShopHydrate {
    shop {
      id
      name
      myshopifyDomain
      url
      currencyCode
      primaryDomain {
        id
        host
        url
        sslEnabled
      }
    }
  }
`;

const customAppBillingProbeMutation = `#graphql
  mutation CustomAppBillingProbe(
    $name: String!
    $returnUrl: URL!
    $lineItems: [AppSubscriptionLineItemInput!]!
  ) {
    appSubscriptionCreate(name: $name, returnUrl: $returnUrl, test: true, lineItems: $lineItems) {
      appSubscription {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function readRecord(value: unknown, context: string): JsonRecord {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${context} expected object, got ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
}

function readArray(value: unknown, context: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`${context} expected array, got ${JSON.stringify(value)}`);
  return value;
}

function payloadData(payload: unknown): JsonRecord {
  return readRecord(readRecord(payload, 'payload')['data'], 'payload.data');
}

function payloadRoot(payload: unknown, rootName: string): JsonRecord {
  return readRecord(payloadData(payload)[rootName], `payload.data.${rootName}`);
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readRecord(result.payload, label)['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, rootName: string, label: string): void {
  const userErrors = readArray(payloadRoot(payload, rootName)['userErrors'], `${label}.userErrors`);
  if (userErrors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertExpectedBillingBlocker(payload: unknown): void {
  const userErrors = readArray(payloadRoot(payload, 'appSubscriptionCreate')['userErrors'], 'billingProbe.userErrors');
  const hasCustomAppBlocker = userErrors.some((error) => {
    const record = readRecord(error, 'billingProbe.userError');
    return record['field'] === null && record['message'] === 'Custom apps cannot use the Billing API';
  });
  if (!hasCustomAppBlocker) {
    throw new Error(`Expected custom-app billing blocker, got ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function currentInstallationIdentity(payload: unknown): JsonRecord {
  const installation = payloadRoot(payload, 'currentAppInstallation');
  const app = readRecord(installation['app'], 'currentAppInstallation.app');
  const identity = {
    id: installation['id'],
    appId: app['id'],
    appHandle: app['handle'],
    appTitle: app['title'],
  };
  for (const [key, value] of Object.entries(identity)) {
    if (typeof value !== 'string' || value.length === 0) {
      throw new Error(`currentAppInstallation missing ${key}: ${JSON.stringify(installation, null, 2)}`);
    }
  }
  return identity;
}

function assertSameIdentity(before: JsonRecord, after: JsonRecord): void {
  for (const key of ['id', 'appId', 'appHandle', 'appTitle']) {
    if (before[key] !== after[key]) {
      throw new Error(`currentAppInstallation identity changed at ${key}: before=${before[key]} after=${after[key]}`);
    }
  }
}

function createdDelegateToken(payload: unknown): string {
  const token = payloadRoot(payload, 'delegateAccessTokenCreate')['delegateAccessToken'];
  const accessToken = readRecord(token, 'delegateAccessTokenCreate.delegateAccessToken')['accessToken'];
  if (typeof accessToken !== 'string' || accessToken.length === 0) {
    throw new Error(`delegateAccessTokenCreate did not return an access token: ${JSON.stringify(token, null, 2)}`);
  }
  return accessToken;
}

function redactDelegateTokenPayload(payload: unknown): unknown {
  return JSON.parse(
    JSON.stringify(payload, (_key, value) =>
      typeof value === 'string' && (value.startsWith('shpat_') || value.startsWith('shpca_'))
        ? '[redacted-live-delegate-token]'
        : value,
    ),
  ) as unknown;
}

async function capture(label: string, query: string, variables: JsonRecord = {}): Promise<Scenario> {
  const result = await runGraphqlRaw(query, variables);
  assertNoTopLevelErrors(result, label);
  return {
    label,
    query,
    variables,
    status: result.status,
    response: result.payload,
  };
}

const suffix = Date.now().toString(36);
const scenarios: Record<string, Scenario> = {};
const cleanup: Scenario[] = [];
let delegateToken: string | null = null;
let delegateTokenDestroyed = false;
let observedIdentity: JsonRecord | null = null;
let readbackIdentity: JsonRecord | null = null;

try {
  scenarios['observedCurrentInstallation'] = await capture('observedCurrentInstallation', readCurrentInstallationQuery);
  observedIdentity = currentInstallationIdentity(scenarios['observedCurrentInstallation'].response);

  scenarios['billingMutationProbe'] = await capture('billingMutationProbe', customAppBillingProbeMutation, {
    name: `Observed identity test plan ${suffix}`,
    returnUrl: 'https://app.example.test/return',
    lineItems: [
      {
        plan: {
          appRecurringPricingDetails: {
            price: { amount: '1.00', currencyCode: 'USD' },
            interval: 'EVERY_30_DAYS',
          },
        },
      },
    ],
  });
  assertExpectedBillingBlocker(scenarios['billingMutationProbe'].response);

  scenarios['createDelegateToken'] = await capture('createDelegateToken', createDelegateTokenMutation);
  assertNoUserErrors(scenarios['createDelegateToken'].response, 'delegateAccessTokenCreate', 'createDelegateToken');
  delegateToken = createdDelegateToken(scenarios['createDelegateToken'].response);
  scenarios['createDelegateToken'].response = redactDelegateTokenPayload(scenarios['createDelegateToken'].response);

  scenarios['shopIdentityHydrate'] = await capture('shopIdentityHydrate', shopIdentityHydrateQuery);

  scenarios['readAfterCreate'] = await capture('readAfterCreate', readCurrentInstallationQuery);
  readbackIdentity = currentInstallationIdentity(scenarios['readAfterCreate'].response);
  assertSameIdentity(observedIdentity, readbackIdentity);
} finally {
  if (delegateToken !== null) {
    const result = await runGraphqlRaw(destroyDelegateTokenMutation, { token: delegateToken });
    delegateTokenDestroyed = result.status >= 200 && result.status < 300 && !result.payload.errors;
    cleanup.push({
      label: 'cleanup delegateAccessTokenDestroy',
      query: destroyDelegateTokenMutation,
      variables: { token: '[redacted-live-delegate-token]' },
      status: result.status,
      response: result.payload,
    });
  }
}

const outputPath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'apps',
  'current-app-installation-observed-identity-local-app-mutation.json',
);

await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      apiVersion,
      storeDomain,
      notes: [
        'Captures the installed app identity returned by currentAppInstallation before and after a real delegateAccessTokenCreate call.',
        'The conformance app is a custom app, so the fixture records Shopify rejecting appSubscriptionCreate with "Custom apps cannot use the Billing API" and uses delegateAccessTokenCreate as the real app-domain mutation that this app can execute.',
        'The capture asserts Shopify keeps currentAppInstallation.id and app identity stable across the app mutation. The created delegate token is destroyed in cleanup when Shopify returns a token.',
        'The upstreamCalls cassette records the exact ProductPayloadShopHydrate query used when the local delegateAccessTokenCreate payload selects shop identity fields.',
      ],
      observedIdentity,
      readbackIdentity,
      delegateTokenDestroyed,
      scenarios,
      cleanup,
      upstreamCalls: [
        {
          operationName: 'ProductPayloadShopHydrate',
          variables: {},
          query: shopIdentityHydrateQuery,
          response: {
            status: scenarios['shopIdentityHydrate'].status,
            body: scenarios['shopIdentityHydrate'].response,
          },
        },
      ],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote current app installation identity fixture to ${outputPath}`);
