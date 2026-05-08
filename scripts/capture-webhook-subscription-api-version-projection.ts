/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
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
const outputPath = path.join(outputDir, 'webhook-subscription-api-version-projection.json');

const requestDir = path.join('config', 'parity-requests', 'webhooks');
const createRequestPath = path.join(requestDir, 'webhook-subscription-api-version-create.graphql');
const detailRequestPath = path.join(requestDir, 'webhook-subscription-api-version-detail.graphql');
const listRequestPath = path.join(requestDir, 'webhook-subscription-api-version-list.graphql');
const deleteRequestPath = path.join(requestDir, 'webhookSubscriptionDelete-parity.graphql');

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
      query FindDisposableWebhookApiVersionProjectionSubscriptions {
        webhookSubscriptions(first: 50, query: "uri:hermes-webhook-api-version") {
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

  const connection = data['webhookSubscriptions'];
  if (!isObject(connection) || !Array.isArray(connection['nodes'])) {
    return [];
  }

  const cleanupResponses: ConformanceGraphqlResult[] = [];
  const deleteDocument = await readText(deleteRequestPath);
  for (const node of connection['nodes']) {
    if (
      isObject(node) &&
      typeof node['id'] === 'string' &&
      typeof node['uri'] === 'string' &&
      node['uri'].includes('hermes-webhook-api-version')
    ) {
      const response = await runGraphqlRequest(deleteDocument, { id: node['id'] });
      requireSuccessfulGraphql(response, `pre-capture cleanup ${node['id']}`);
      cleanupResponses.push(response);
    }
  }

  return cleanupResponses;
}

function readCreatedWebhookId(captureResult: CapturedRequest): string {
  const data = captureResult.response.payload.data;
  if (!isObject(data)) {
    throw new Error('webhookSubscriptionCreate response did not include data.');
  }

  const payload = data['webhookSubscriptionCreate'];
  if (!isObject(payload)) {
    throw new Error('webhookSubscriptionCreate payload is missing.');
  }

  const subscription = payload['webhookSubscription'];
  if (!isObject(subscription) || typeof subscription['id'] !== 'string') {
    throw new Error('webhookSubscriptionCreate did not return a webhookSubscription.id.');
  }

  const apiVersionValue = subscription['apiVersion'];
  if (
    !isObject(apiVersionValue) ||
    typeof apiVersionValue['handle'] !== 'string' ||
    apiVersionValue['handle'].length === 0
  ) {
    throw new Error(`webhookSubscriptionCreate returned invalid apiVersion: ${JSON.stringify(apiVersionValue)}`);
  }

  return subscription['id'];
}

function requireApiVersionAt(pathLabel: string, value: unknown): void {
  if (
    !isObject(value) ||
    !isObject(value['apiVersion']) ||
    typeof value['apiVersion']['handle'] !== 'string' ||
    value['apiVersion']['handle'].length === 0
  ) {
    throw new Error(`${pathLabel} did not return a non-empty apiVersion: ${JSON.stringify(value)}`);
  }
}

function assertCapturedApiVersions(lifecycle: {
  create: CapturedRequest | null;
  detail: CapturedRequest | null;
  connection: CapturedRequest | null;
}): void {
  const createData = lifecycle.create?.response.payload.data;
  if (!isObject(createData)) {
    throw new Error('create data missing during apiVersion assertion.');
  }
  const createPayload = createData['webhookSubscriptionCreate'];
  if (!isObject(createPayload)) {
    throw new Error('create payload missing during apiVersion assertion.');
  }
  requireApiVersionAt('create webhookSubscription', createPayload['webhookSubscription']);

  const detailData = lifecycle.detail?.response.payload.data;
  if (!isObject(detailData)) {
    throw new Error('detail data missing during apiVersion assertion.');
  }
  requireApiVersionAt('detail webhookSubscription', detailData['webhookSubscription']);

  const connectionData = lifecycle.connection?.response.payload.data;
  if (!isObject(connectionData)) {
    throw new Error('connection data missing during apiVersion assertion.');
  }
  const connection = connectionData['webhookSubscriptions'];
  if (!isObject(connection) || !Array.isArray(connection['nodes']) || connection['nodes'].length !== 1) {
    throw new Error(`connection did not return exactly one node: ${JSON.stringify(connection)}`);
  }
  requireApiVersionAt('connection node', connection['nodes'][0]);
}

const runId = Date.now().toString(36);
const uri = `https://example.com/hermes-webhook-api-version-${runId}`;
const createVariables = {
  topic: 'SHOP_UPDATE',
  webhookSubscription: {
    uri,
    format: 'JSON',
    includeFields: ['id'],
    metafieldNamespaces: [],
    filter: '',
  },
};

let createdId: string | null = null;
let cleanup: CapturedRequest | null = null;
const lifecycle: {
  create: CapturedRequest | null;
  detail: CapturedRequest | null;
  connection: CapturedRequest | null;
  delete: CapturedRequest | null;
} = {
  create: null,
  detail: null,
  connection: null,
  delete: null,
};

const preCaptureCleanup = await cleanupExistingDisposableWebhooks();

try {
  lifecycle.create = await capture(createRequestPath, createVariables);
  createdId = readCreatedWebhookId(lifecycle.create);
  lifecycle.detail = await capture(detailRequestPath, { id: createdId });
  lifecycle.connection = await capture(listRequestPath, { first: 5, uri });
  assertCapturedApiVersions(lifecycle);
  lifecycle.delete = await capture(deleteRequestPath, { id: createdId });
  cleanup = lifecycle.delete;

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        notes: [
          'Captures WebhookSubscription.apiVersion on the create payload, detail read, and filtered connection node for an API-created HTTP webhook subscription.',
          'The temporary subscription is deleted during cleanup and the script does not trigger webhook delivery.',
        ],
        preCaptureCleanup,
        lifecycle,
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
} finally {
  if (createdId !== null && cleanup === null) {
    await capture(deleteRequestPath, { id: createdId });
  }
}

console.log(`Wrote webhook apiVersion fixture to ${outputPath}`);
