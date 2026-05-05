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
const outputPath = path.join(outputDir, 'webhook-subscription-uri-validation.json');

const primaryDocumentPath = path.join(
  'config',
  'parity-requests',
  'webhooks',
  'webhook-subscription-uri-validation.graphql',
);
const updateDocumentPath = path.join(
  'config',
  'parity-requests',
  'webhooks',
  'webhookSubscriptionUpdate-parity.graphql',
);
const deleteDocumentPath = path.join(
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

async function cleanupExistingDisposableWebhooks(): Promise<ConformanceGraphqlResult[]> {
  const findResponse = await runGraphqlRequest(
    `#graphql
      query FindDisposableWebhookUriValidationSubscriptions {
        webhookSubscriptions(first: 50, query: "uri:hermes-webhook-uri-validation") {
          nodes {
            id
            uri
          }
        }
      }
    `,
    {},
  );
  requireSuccessfulGraphql(findResponse, 'pre-capture disposable webhook lookup');

  const data = findResponse.payload.data;
  if (!isObject(data)) {
    return [];
  }

  const nodes = data['webhookSubscriptions'];
  if (!isObject(nodes) || !Array.isArray(nodes['nodes'])) {
    return [];
  }

  const cleanupResponses: ConformanceGraphqlResult[] = [];
  const deleteDocument = await readText(deleteDocumentPath);
  for (const node of nodes['nodes']) {
    if (
      isObject(node) &&
      typeof node['id'] === 'string' &&
      typeof node['uri'] === 'string' &&
      node['uri'].includes('hermes-webhook-uri-validation')
    ) {
      const response = await runGraphqlRequest(deleteDocument, { id: node['id'] });
      requireSuccessfulGraphql(response, `pre-capture cleanup ${node['id']}`);
      cleanupResponses.push(response);
    }
  }

  return cleanupResponses;
}

function readBaselineWebhookId(primary: CapturedRequest): string | null {
  const data = primary.response.payload.data;
  if (!isObject(data)) {
    return null;
  }

  const payload = data['baselineCreate'];
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

function assertUserError(capture: CapturedRequest, path: string[], expectedMessagePrefix: string): void {
  let current: unknown = capture.response.payload;
  for (const segment of path) {
    if (!isObject(current)) {
      throw new Error(`${capture.documentPath} missing object at ${segment}`);
    }
    current = current[segment];
  }

  if (!isObject(current)) {
    throw new Error(`${capture.documentPath} missing payload at ${path.join('.')}`);
  }

  const webhookSubscription = current['webhookSubscription'];
  const userErrors = current['userErrors'];
  if (webhookSubscription !== null || !Array.isArray(userErrors) || userErrors.length !== 1) {
    throw new Error(`${capture.documentPath} did not reject as expected: ${JSON.stringify(current)}`);
  }

  const error = userErrors[0];
  if (!isObject(error) || !Array.isArray(error['field']) || typeof error['message'] !== 'string') {
    throw new Error(`${capture.documentPath} returned malformed userError: ${JSON.stringify(error)}`);
  }

  const field = JSON.stringify(error['field']);
  if (field !== JSON.stringify(['webhookSubscription', 'callbackUrl'])) {
    throw new Error(`${capture.documentPath} returned unexpected field ${field}`);
  }

  if (!error['message'].startsWith(expectedMessagePrefix)) {
    throw new Error(`${capture.documentPath} returned unexpected message ${error['message']}`);
  }
}

const suffix = `${Date.now()}`;
const baselineVariables = {
  baselineTopic: 'SHOP_UPDATE',
  baselineWebhookSubscription: {
    filter: '',
    format: 'JSON',
    includeFields: ['id'],
    metafieldNamespaces: [],
    uri: `https://example.com/hermes-webhook-uri-validation-${suffix}`,
  },
};

let createdId: string | null = null;
let cleanup: CapturedRequest | null = null;
let updateHttp: CapturedRequest | null = null;
const preCaptureCleanup = await cleanupExistingDisposableWebhooks();
const primary = await capture(primaryDocumentPath, baselineVariables);
createdId = readBaselineWebhookId(primary);

try {
  assertUserError(primary, ['data', 'createHttpUriRejected'], 'Address protocol http:// is not supported');
  assertUserError(
    primary,
    ['data', 'createInternalDomainRejected'],
    'Address cannot be a Shopify or an internal domain',
  );
  assertUserError(
    primary,
    ['data', 'createShopifyDomainRejected'],
    'Address cannot be a Shopify or an internal domain',
  );

  if (createdId === null) {
    throw new Error('Baseline webhookSubscriptionCreate did not return an id.');
  }

  updateHttp = await capture(updateDocumentPath, {
    id: createdId,
    webhookSubscription: {
      format: 'JSON',
      includeFields: ['id'],
      metafieldNamespaces: [],
      uri: 'http://example.com/hook',
    },
  });
  assertUserError(updateHttp, ['data', 'webhookSubscriptionUpdate'], 'Address protocol http:// is not supported');
} finally {
  if (createdId !== null) {
    cleanup = await capture(deleteDocumentPath, { id: createdId });
  }
}

if (updateHttp === null) {
  throw new Error('webhookSubscriptionUpdate URI validation was not captured.');
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
        'HAR-719 captures webhookSubscriptionCreate and webhookSubscriptionUpdate URI validation userErrors against live Shopify.',
        'The baseline subscription is created only so the invalid update can target a real ID; it is deleted during cleanup.',
        'No webhook delivery is intentionally triggered by this validation capture.',
      ],
      preCaptureCleanup,
      primary,
      updateHttp,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote webhook subscription URI validation fixture to ${outputPath}`);
