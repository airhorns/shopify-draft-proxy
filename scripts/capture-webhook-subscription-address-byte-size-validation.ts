/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedRequest = {
  documentPath: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

const maxAddressBytes = 65_535;
const uriPrefix = 'https://example.com/';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'webhooks');
const outputPath = path.join(outputDir, 'webhook-subscription-address-byte-size-validation.json');
const specPath = path.join(
  'config',
  'parity-specs',
  'webhooks',
  'webhook-subscription-address-byte-size-validation.json',
);

const createRequestPath = path.join(
  'config',
  'parity-requests',
  'webhooks',
  'webhookSubscriptionCreate-parity.graphql',
);
const deleteRequestPath = path.join(
  'config',
  'parity-requests',
  'webhooks',
  'webhookSubscriptionDelete-parity.graphql',
);

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readText(relativePath: string): Promise<string> {
  return readFile(path.join(process.cwd(), relativePath), 'utf8');
}

function sizedHttpsUri(byteSize: number): string {
  const suffixBytes = byteSize - Buffer.byteLength(uriPrefix, 'utf8');
  if (suffixBytes < 1) {
    throw new Error(`URI byte size ${byteSize} is too small for ${uriPrefix}`);
  }
  return `${uriPrefix}${'a'.repeat(suffixBytes)}`;
}

function createVariables(uri: string): Record<string, unknown> {
  return {
    topic: 'SHOP_UPDATE',
    webhookSubscription: {
      uri,
      format: 'JSON',
    },
  };
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function requireSuccessfulGraphql(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result.payload)}`);
  }
}

async function capture(documentPath: string, variables: Record<string, unknown>): Promise<CapturedRequest> {
  const document = await readText(documentPath);
  const response = await runGraphqlRequest(document, variables);
  requireSuccessfulGraphql(response, documentPath);

  return { documentPath, variables, response };
}

function readCreatedWebhookId(captureResult: CapturedRequest): string | null {
  const data = captureResult.response.payload.data;
  if (!isObject(data)) return null;

  const payload = data['webhookSubscriptionCreate'];
  if (!isObject(payload)) return null;

  const webhookSubscription = payload['webhookSubscription'];
  if (!isObject(webhookSubscription)) return null;

  const id = webhookSubscription['id'];
  return typeof id === 'string' ? id : null;
}

function assertCreated(captureResult: CapturedRequest): string {
  const id = readCreatedWebhookId(captureResult);
  if (id === null) {
    throw new Error(
      `Expected accepted address capture to create a subscription: ${JSON.stringify(captureResult.response.payload)}`,
    );
  }

  const data = captureResult.response.payload.data;
  if (!isObject(data)) throw new Error('Accepted address capture did not return data.');
  const payload = data['webhookSubscriptionCreate'];
  if (!isObject(payload) || !Array.isArray(payload['userErrors']) || payload['userErrors'].length !== 0) {
    throw new Error(`Accepted address capture returned userErrors: ${JSON.stringify(payload)}`);
  }

  return id;
}

function assertAddressTooLong(captureResult: CapturedRequest): void {
  const data = captureResult.response.payload.data;
  if (!isObject(data)) throw new Error('Too-long address capture did not return data.');
  const payload = data['webhookSubscriptionCreate'];
  if (!isObject(payload)) throw new Error('Too-long address capture is missing payload.');
  if (payload['webhookSubscription'] !== null) {
    throw new Error(`Too-long address unexpectedly created a subscription: ${JSON.stringify(payload)}`);
  }
  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 1 || !isObject(userErrors[0])) {
    throw new Error(`Too-long address returned malformed userErrors: ${JSON.stringify(payload)}`);
  }
  const error = userErrors[0];
  if (
    JSON.stringify(error['field']) !== JSON.stringify(['webhookSubscription', 'callbackUrl']) ||
    error['message'] !== 'Address is too big (maximum is 64 KB)'
  ) {
    throw new Error(`Too-long address returned unexpected userError: ${JSON.stringify(error)}`);
  }
}

const acceptedAtLimit = await capture(createRequestPath, createVariables(sizedHttpsUri(maxAddressBytes)));
const createdId = assertCreated(acceptedAtLimit);

let cleanup: CapturedRequest | null = null;
try {
  const rejectedAboveLimit = await capture(createRequestPath, createVariables(sizedHttpsUri(maxAddressBytes + 1)));
  assertAddressTooLong(rejectedAboveLimit);

  cleanup = await capture(deleteRequestPath, { id: createdId });

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        notes: [
          'Captures Shopify webhookSubscriptionCreate address byte-size validation at the MySQL text-column limit.',
          `The accepted case uses an HTTPS URI with exactly ${maxAddressBytes} bytes and is deleted during cleanup.`,
          `The rejected case uses an HTTPS URI with ${maxAddressBytes + 1} bytes and returns Address is too big (maximum is 64 KB).`,
          'No webhook delivery is intentionally triggered by this validation capture.',
        ],
        cases: {
          acceptedAtLimit,
          rejectedAboveLimit,
        },
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
  await writeFile(specPath, `${JSON.stringify(buildSpec(), null, 2)}\n`, 'utf8');
} finally {
  if (cleanup === null) {
    await capture(deleteRequestPath, { id: createdId });
  }
}

console.log(`Wrote webhook subscription address byte-size fixture to ${outputPath}`);
console.log(`Wrote webhook subscription address byte-size parity spec to ${specPath}`);

function buildSpec(): Record<string, unknown> {
  return {
    scenarioId: 'webhook-subscription-address-byte-size-validation',
    operationNames: ['webhookSubscriptionCreate'],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity', 'payload-shape'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['test/parity_test.gleam', 'test/shopify_draft_proxy/proxy/webhooks_test.gleam'],
    proxyRequest: {
      documentPath: createRequestPath,
      apiVersion,
      variablesCapturePath: '$.cases.acceptedAtLimit.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Strict parity for webhookSubscriptionCreate address byte-size validation. Shopify accepts a valid HTTPS callback URL at 65,535 bytes and rejects the same shape at 65,536 bytes with Address is too big (maximum is 64 KB).',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'accepted-at-limit',
          capturePath: '$.cases.acceptedAtLimit.response.payload.data.webhookSubscriptionCreate',
          proxyPath: '$.data.webhookSubscriptionCreate',
          expectedDifferences: [
            {
              path: '$.webhookSubscription.id',
              matcher: 'shopify-gid:WebhookSubscription',
              reason: "The proxy creates a stable synthetic webhook subscription ID instead of Shopify's live ID.",
            },
            {
              path: '$.webhookSubscription.createdAt',
              matcher: 'iso-timestamp',
              reason: 'The proxy uses its deterministic synthetic clock for locally staged webhook subscriptions.',
            },
            {
              path: '$.webhookSubscription.updatedAt',
              matcher: 'iso-timestamp',
              reason: 'The proxy uses its deterministic synthetic clock for locally staged webhook subscriptions.',
            },
          ],
        },
        {
          name: 'rejected-above-limit',
          capturePath: '$.cases.rejectedAboveLimit.response.payload.data.webhookSubscriptionCreate',
          proxyPath: '$.data.webhookSubscriptionCreate',
          proxyRequest: {
            documentPath: createRequestPath,
            variablesCapturePath: '$.cases.rejectedAboveLimit.variables',
          },
        },
      ],
    },
  };
}
