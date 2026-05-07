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
const outputPath = path.join(outputDir, 'webhook-subscription-metafield-namespaces-resolution.json');
const specPath = path.join(
  'config',
  'parity-specs',
  'webhooks',
  'webhook-subscription-metafield-namespaces-resolution.json',
);

const requestDir = path.join('config', 'parity-requests', 'webhooks');
const createRequestPath = path.join(requestDir, 'webhookSubscriptionCreate-parity.graphql');
const updateRequestPath = path.join(requestDir, 'webhookSubscriptionUpdate-parity.graphql');
const detailRequestPath = path.join(requestDir, 'webhook-subscription-detail-read.graphql');
const listRequestPath = path.join(requestDir, 'webhook-subscription-metafield-namespaces-list.graphql');
const deleteRequestPath = path.join(requestDir, 'webhookSubscriptionDelete-parity.graphql');

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

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
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

  return subscription['id'];
}

function readMetafieldNamespaces(captureResult: CapturedRequest, rootName: string): string[] {
  const data = captureResult.response.payload.data;
  if (!isObject(data)) {
    throw new Error(`${rootName} response did not include data.`);
  }

  const payload = data[rootName];
  if (!isObject(payload)) {
    throw new Error(`${rootName} payload is missing.`);
  }

  const subscription = payload['webhookSubscription'];
  if (!isObject(subscription) || !Array.isArray(subscription['metafieldNamespaces'])) {
    throw new Error(`${rootName} did not return webhookSubscription.metafieldNamespaces.`);
  }

  return subscription['metafieldNamespaces'].filter((value): value is string => typeof value === 'string');
}

function deriveRequestingApiClientId(namespaces: string[]): string {
  const resolved = namespaces.find((namespace) => namespace.startsWith('app--'));
  const match = resolved?.match(/^app--([^-]+)--/u);
  if (!match?.[1]) {
    throw new Error(`Could not derive requesting api_client_id from namespaces: ${JSON.stringify(namespaces)}`);
  }

  return match[1];
}

function createVariables(uri: string, runId: string): Record<string, unknown> {
  return {
    topic: 'PRODUCTS_UPDATE',
    webhookSubscription: {
      uri,
      format: 'JSON',
      includeFields: ['id'],
      metafieldNamespaces: [`$app:Settings${runId}`, 'custom', `app--999999999999--kept-${runId}`],
    },
  };
}

function updateVariables(id: string, uri: string, runId: string): Record<string, unknown> {
  return {
    id,
    webhookSubscription: {
      uri,
      format: 'JSON',
      includeFields: ['id'],
      metafieldNamespaces: [`$app:Billing${runId}`, 'custom'],
    },
  };
}

const runId = Date.now().toString(36);
const createUri = `https://example.com/hermes-webhook-namespace-${runId}`;
const updateUri = `${createUri}-updated`;

let createdId: string | null = null;
let cleanup: CapturedRequest | null = null;
const lifecycle: {
  create: CapturedRequest | null;
  detailAfterCreate: CapturedRequest | null;
  listAfterCreate: CapturedRequest | null;
  update: CapturedRequest | null;
  detailAfterUpdate: CapturedRequest | null;
  listAfterUpdate: CapturedRequest | null;
  delete: CapturedRequest | null;
} = {
  create: null,
  detailAfterCreate: null,
  listAfterCreate: null,
  update: null,
  detailAfterUpdate: null,
  listAfterUpdate: null,
  delete: null,
};

try {
  lifecycle.create = await capture(createRequestPath, createVariables(createUri, runId));
  createdId = readCreatedWebhookId(lifecycle.create);
  const createNamespaces = readMetafieldNamespaces(lifecycle.create, 'webhookSubscriptionCreate');
  const requestingApiClientId = deriveRequestingApiClientId(createNamespaces);

  lifecycle.detailAfterCreate = await capture(detailRequestPath, { id: createdId });
  lifecycle.listAfterCreate = await capture(listRequestPath, { first: 5, uri: createUri });
  lifecycle.update = await capture(updateRequestPath, updateVariables(createdId, updateUri, runId));
  lifecycle.detailAfterUpdate = await capture(detailRequestPath, { id: createdId });
  lifecycle.listAfterUpdate = await capture(listRequestPath, { first: 5, uri: updateUri });
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
        requestingApiClientId,
        notes: [
          'Captures webhookSubscriptionCreate/update metafieldNamespaces app-namespace resolution for API-created HTTP webhook subscriptions.',
          'The create branch uses a mixed-case `$app:Settings...` shorthand plus non-app values; Shopify stores and returns the canonical app namespace while preserving non-app entries.',
          'The update branch proves the same resolution path before downstream detail and filtered list reads.',
          'The temporary subscription is deleted during cleanup and the script does not trigger webhook delivery.',
        ],
        expectedResolvedNamespaces: {
          create: createNamespaces,
          update: readMetafieldNamespaces(lifecycle.update, 'webhookSubscriptionUpdate'),
        },
        lifecycle,
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  await writeFile(specPath, `${JSON.stringify(buildSpec(requestingApiClientId), null, 2)}\n`, 'utf8');
} finally {
  if (createdId !== null && cleanup === null) {
    await capture(deleteRequestPath, { id: createdId });
  }
}

console.log(`Wrote webhook metafield namespace fixture to ${outputPath}`);
console.log(`Wrote webhook metafield namespace parity spec to ${specPath}`);

function buildSpec(requestingApiClientId: string): Record<string, unknown> {
  const headers = {
    'x-shopify-draft-proxy-api-client-id': requestingApiClientId,
  };

  const idDifference = {
    path: '$.webhookSubscription.id',
    matcher: 'shopify-gid:WebhookSubscription',
    reason: "The proxy creates a stable synthetic webhook subscription ID instead of Shopify's live ID.",
  };
  const createdAtDifference = {
    path: '$.webhookSubscription.createdAt',
    matcher: 'iso-timestamp',
    reason: 'The proxy uses its deterministic synthetic clock for locally staged webhook subscriptions.',
  };
  const updatedAtDifference = {
    path: '$.webhookSubscription.updatedAt',
    matcher: 'iso-timestamp',
    reason: 'The proxy uses its deterministic synthetic clock for locally staged webhook subscriptions.',
  };
  const nodeIdDifference = {
    path: '$.nodes[0].id',
    matcher: 'shopify-gid:WebhookSubscription',
    reason: "The proxy-created webhook subscription ID differs from Shopify's live ID.",
  };

  return {
    scenarioId: 'webhook-subscription-metafield-namespaces-resolution',
    operationNames: [
      'webhookSubscriptionCreate',
      'webhookSubscriptionUpdate',
      'webhookSubscription',
      'webhookSubscriptions',
    ],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'read-after-write', 'downstream-read-parity'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['test/parity_test.gleam', 'test/shopify_draft_proxy/proxy/webhooks_test.gleam'],
    proxyRequest: {
      documentPath: createRequestPath,
      variablesCapturePath: '$.lifecycle.create.variables',
      apiVersion,
      headers,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Strict parity for webhookSubscriptionCreate/update metafieldNamespaces app-namespace resolution. The scenario records real Shopify canonicalization for `$app:` inputs and replays create/update plus downstream detail and filtered list reads with the request api-client identity header.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'create-payload',
          capturePath: '$.lifecycle.create.response.payload.data.webhookSubscriptionCreate',
          proxyPath: '$.data.webhookSubscriptionCreate',
          expectedDifferences: [idDifference, createdAtDifference, updatedAtDifference],
        },
        {
          name: 'detail-after-create',
          capturePath: '$.lifecycle.detailAfterCreate.response.payload.data.webhookSubscription',
          proxyPath: '$.data.webhookSubscription',
          proxyRequest: {
            documentPath: detailRequestPath,
            variables: {
              id: {
                fromPrimaryProxyPath: '$.data.webhookSubscriptionCreate.webhookSubscription.id',
              },
            },
            apiVersion,
            headers,
          },
          expectedDifferences: [
            {
              path: '$.id',
              matcher: 'shopify-gid:WebhookSubscription',
              reason: 'The downstream read sees the proxy-created synthetic webhook subscription ID.',
            },
            {
              path: '$.createdAt',
              matcher: 'iso-timestamp',
              reason: 'The downstream read sees the proxy synthetic create timestamp.',
            },
            {
              path: '$.updatedAt',
              matcher: 'iso-timestamp',
              reason: 'The downstream read sees the proxy synthetic update timestamp.',
            },
          ],
        },
        {
          name: 'list-after-create',
          capturePath: '$.lifecycle.listAfterCreate.response.payload.data.webhookSubscriptions',
          proxyPath: '$.data.webhookSubscriptions',
          proxyRequest: {
            documentPath: listRequestPath,
            variablesCapturePath: '$.lifecycle.listAfterCreate.variables',
            apiVersion,
            headers,
          },
          expectedDifferences: [nodeIdDifference],
        },
        {
          name: 'update-payload',
          capturePath: '$.lifecycle.update.response.payload.data.webhookSubscriptionUpdate',
          proxyPath: '$.data.webhookSubscriptionUpdate',
          proxyRequest: {
            documentPath: updateRequestPath,
            variables: {
              id: {
                fromPrimaryProxyPath: '$.data.webhookSubscriptionCreate.webhookSubscription.id',
              },
              webhookSubscription: {
                fromCapturePath: '$.lifecycle.update.variables.webhookSubscription',
              },
            },
            apiVersion,
            headers,
          },
          expectedDifferences: [idDifference, createdAtDifference, updatedAtDifference],
        },
        {
          name: 'detail-after-update',
          capturePath: '$.lifecycle.detailAfterUpdate.response.payload.data.webhookSubscription',
          proxyPath: '$.data.webhookSubscription',
          proxyRequest: {
            documentPath: detailRequestPath,
            variables: {
              id: {
                fromPrimaryProxyPath: '$.data.webhookSubscriptionCreate.webhookSubscription.id',
              },
            },
            apiVersion,
            headers,
          },
          expectedDifferences: [
            {
              path: '$.id',
              matcher: 'shopify-gid:WebhookSubscription',
              reason: 'The downstream read sees the proxy-created synthetic webhook subscription ID.',
            },
            {
              path: '$.createdAt',
              matcher: 'iso-timestamp',
              reason: 'The downstream read keeps the proxy synthetic create timestamp.',
            },
            {
              path: '$.updatedAt',
              matcher: 'iso-timestamp',
              reason: 'The downstream read sees the proxy synthetic update timestamp.',
            },
          ],
        },
        {
          name: 'list-after-update',
          capturePath: '$.lifecycle.listAfterUpdate.response.payload.data.webhookSubscriptions',
          proxyPath: '$.data.webhookSubscriptions',
          proxyRequest: {
            documentPath: listRequestPath,
            variablesCapturePath: '$.lifecycle.listAfterUpdate.variables',
            apiVersion,
            headers,
          },
          expectedDifferences: [nodeIdDifference],
        },
      ],
    },
  };
}
