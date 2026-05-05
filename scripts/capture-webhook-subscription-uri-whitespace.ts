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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'webhooks');
const outputPath = path.join(outputDir, 'webhook-subscription-uri-whitespace.json');
const specPath = path.join('config', 'parity-specs', 'webhooks', 'webhook-subscription-uri-whitespace.json');

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

async function capture(documentPath: string, variables: Record<string, unknown>): Promise<CapturedRequest> {
  const document = await readText(documentPath);
  const response = await runGraphqlRequest(document, variables);

  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`${documentPath} failed: ${JSON.stringify(response.payload)}`);
  }

  return { documentPath, variables, response };
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

function readCreatedWebhookId(captureResult: CapturedRequest): string | null {
  const data = captureResult.response.payload.data;
  if (!isObject(data)) {
    return null;
  }

  const payload = data['webhookSubscriptionCreate'];
  if (!isObject(payload)) {
    return null;
  }

  const webhookSubscription = payload['webhookSubscription'];
  if (!isObject(webhookSubscription)) {
    return null;
  }

  const id = webhookSubscription['id'];
  return typeof id === 'string' ? id : null;
}

const cases = {
  createWhitespaceOnlyUri: await capture(createRequestPath, createVariables('   ')),
  createUriWithLeadingWhitespace: await capture(createRequestPath, createVariables('  https://example.com/h  ')),
};

const cleanup: Record<string, CapturedRequest> = {};
for (const [name, captureResult] of Object.entries(cases)) {
  const createdId = readCreatedWebhookId(captureResult);
  if (createdId !== null) {
    cleanup[name] = await capture(deleteRequestPath, { id: createdId });
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'HAR-730 captures webhookSubscriptionCreate URI whitespace validation.',
        'Current Shopify 2026-04 trims leading/trailing whitespace from otherwise-valid HTTPS uri input, while whitespace-only uri is treated as blank.',
        'Any webhook subscription created by the trimmed HTTPS case is deleted during cleanup; the script does not trigger webhook delivery.',
      ],
      deliveryPolicy: {
        deliveriesTriggeredByScript: false,
      },
      cases,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

await writeFile(specPath, `${JSON.stringify(buildSpec(), null, 2)}\n`, 'utf8');

console.log(`Wrote webhook subscription URI whitespace fixture to ${outputPath}`);
console.log(`Wrote webhook subscription URI whitespace parity spec to ${specPath}`);

function buildSpec(): Record<string, unknown> {
  return {
    scenarioId: 'webhook-subscription-uri-whitespace',
    operationNames: ['webhookSubscriptionCreate'],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['test/parity_test.gleam', 'test/shopify_draft_proxy/proxy/webhooks_test.gleam'],
    proxyRequest: {
      documentPath: createRequestPath,
      apiVersion,
      variablesCapturePath: '$.cases.createWhitespaceOnlyUri.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Strict parity for webhookSubscriptionCreate URI whitespace handling. Live Shopify 2026-04 treats whitespace-only uri as blank and trims leading/trailing whitespace from an otherwise-valid HTTPS uri before storing and returning the subscription.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        createTarget('create-whitespace-only-uri', 'createWhitespaceOnlyUri'),
        createTarget('create-uri-with-leading-whitespace', 'createUriWithLeadingWhitespace'),
      ],
    },
  };
}

function createTarget(name: string, caseName: string): Record<string, unknown> {
  const target: Record<string, unknown> = {
    name,
    capturePath: `$.cases.${caseName}.response.payload.data.webhookSubscriptionCreate`,
    proxyPath: '$.data.webhookSubscriptionCreate',
    proxyRequest: {
      documentPath: createRequestPath,
      variablesCapturePath: `$.cases.${caseName}.variables`,
    },
  };

  if (caseName === 'createUriWithLeadingWhitespace') {
    target['expectedDifferences'] = [
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
    ];
  }

  return target;
}
