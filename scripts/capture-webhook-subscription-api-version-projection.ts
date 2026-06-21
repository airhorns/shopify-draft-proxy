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

type CapturedApiVersion = {
  handle: string;
  displayName: string;
  supported: boolean;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'webhooks');
const outputPath = path.join(outputDir, 'webhook-subscription-api-version-projection.json');
const specPath = path.join('config', 'parity-specs', 'webhooks', 'webhook-subscription-api-version-projection.json');

const requestDir = path.join('config', 'parity-requests', 'webhooks');
const createRequestPath = path.join(requestDir, 'webhook-subscription-api-version-create.graphql');
const updateRequestPath = path.join(requestDir, 'webhook-subscription-api-version-update.graphql');
const detailRequestPath = path.join(requestDir, 'webhook-subscription-api-version-detail-read.graphql');
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

function readWebhookSubscription(captureResult: CapturedRequest, rootName: string): Record<string, unknown> {
  const data = captureResult.response.payload.data;
  if (!isObject(data)) {
    throw new Error(`${rootName} response did not include data.`);
  }

  const root = data[rootName];
  if (!isObject(root)) {
    throw new Error(`${rootName} payload is missing.`);
  }

  if ('webhookSubscription' in root) {
    const subscription = root['webhookSubscription'];
    if (isObject(subscription)) {
      return subscription;
    }
    throw new Error(`${rootName} did not return a webhookSubscription object.`);
  }

  return root;
}

function readCreatedWebhookId(captureResult: CapturedRequest): string {
  const subscription = readWebhookSubscription(captureResult, 'webhookSubscriptionCreate');
  if (typeof subscription['id'] !== 'string') {
    throw new Error('webhookSubscriptionCreate did not return webhookSubscription.id.');
  }
  return subscription['id'];
}

function readApiVersion(captureResult: CapturedRequest, rootName: string): CapturedApiVersion {
  const subscription = readWebhookSubscription(captureResult, rootName);
  const apiVersionValue = subscription['apiVersion'];
  if (!isObject(apiVersionValue)) {
    throw new Error(`${rootName} did not return webhookSubscription.apiVersion.`);
  }

  const { handle, displayName, supported } = apiVersionValue;
  if (typeof handle !== 'string' || typeof displayName !== 'string' || typeof supported !== 'boolean') {
    throw new Error(`${rootName} returned an invalid apiVersion object: ${JSON.stringify(apiVersionValue)}`);
  }

  return { handle, displayName, supported };
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

function updateVariables(id: string, uri: string): Record<string, unknown> {
  return {
    id,
    webhookSubscription: {
      uri,
      format: 'JSON',
    },
  };
}

const runId = Date.now().toString(36);
const createUri = `https://example.com/hermes-webhook-api-version-${runId}`;
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
  lifecycle.create = await capture(createRequestPath, createVariables(createUri));
  createdId = readCreatedWebhookId(lifecycle.create);
  const capturedApiVersion = readApiVersion(lifecycle.create, 'webhookSubscriptionCreate');

  lifecycle.detailAfterCreate = await capture(detailRequestPath, { id: createdId });
  lifecycle.listAfterCreate = await capture(listRequestPath, { first: 5, uri: createUri });
  lifecycle.update = await capture(updateRequestPath, updateVariables(createdId, updateUri));
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
        capturedApiVersion,
        notes: [
          'Captures WebhookSubscription.apiVersion projection for API-created HTTP webhook subscriptions.',
          'The lifecycle records create, detail read, connection node read, update preservation, and delete cleanup.',
          'The temporary subscription is deleted during cleanup and the script does not trigger webhook delivery.',
        ],
        lifecycle,
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  await writeFile(specPath, `${JSON.stringify(buildSpec(capturedApiVersion), null, 2)}\n`, 'utf8');
} finally {
  if (createdId !== null && cleanup === null) {
    await capture(deleteRequestPath, { id: createdId });
  }
}

console.log(`Wrote webhook apiVersion fixture to ${outputPath}`);
console.log(`Wrote webhook apiVersion parity spec to ${specPath}`);

function buildSpec(capturedApiVersion: CapturedApiVersion): Record<string, unknown> {
  const headers = {
    'x-shopify-draft-proxy-api-version': capturedApiVersion.handle,
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
  const detailIdDifference = {
    path: '$.id',
    matcher: 'shopify-gid:WebhookSubscription',
    reason: 'The downstream read sees the proxy-created synthetic webhook subscription ID.',
  };
  const detailCreatedAtDifference = {
    path: '$.createdAt',
    matcher: 'iso-timestamp',
    reason: 'The downstream read sees the proxy synthetic create timestamp.',
  };
  const detailUpdatedAtDifference = {
    path: '$.updatedAt',
    matcher: 'iso-timestamp',
    reason: 'The downstream read sees the proxy synthetic update timestamp.',
  };
  const nodeIdDifference = {
    path: '$.nodes[0].id',
    matcher: 'shopify-gid:WebhookSubscription',
    reason: "The proxy-created webhook subscription ID differs from Shopify's live ID.",
  };

  return {
    scenarioId: 'webhook-subscription-api-version-projection',
    operationNames: [
      'webhookSubscriptionCreate',
      'webhookSubscriptionUpdate',
      'webhookSubscription',
      'webhookSubscriptions',
      'webhookSubscriptionDelete',
    ],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'read-after-write', 'downstream-read-parity'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['tests/graphql_routes/admin_graphql_webhooks.rs'],
    proxyRequest: {
      documentPath: createRequestPath,
      variablesCapturePath: '$.lifecycle.create.variables',
      apiVersion,
      headers,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Strict parity for WebhookSubscription.apiVersion projection. The scenario records the app effective API version returned by Shopify and replays create/update plus downstream detail and connection-node reads with the same draft-proxy API-version identity header.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'create-payload-api-version',
          capturePath: '$.lifecycle.create.response.payload.data.webhookSubscriptionCreate',
          proxyPath: '$.data.webhookSubscriptionCreate',
          expectedDifferences: [idDifference, createdAtDifference, updatedAtDifference],
        },
        {
          name: 'detail-after-create-api-version',
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
          expectedDifferences: [detailIdDifference, detailCreatedAtDifference, detailUpdatedAtDifference],
        },
        {
          name: 'connection-node-after-create-api-version',
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
          name: 'update-payload-api-version',
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
          name: 'detail-after-update-api-version',
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
          expectedDifferences: [detailIdDifference, detailCreatedAtDifference, detailUpdatedAtDifference],
        },
        {
          name: 'connection-node-after-update-api-version',
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
        {
          name: 'delete-payload',
          capturePath: '$.lifecycle.delete.response.payload.data.webhookSubscriptionDelete',
          proxyPath: '$.data.webhookSubscriptionDelete',
          proxyRequest: {
            documentPath: deleteRequestPath,
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
              path: '$.deletedWebhookSubscriptionId',
              matcher: 'shopify-gid:WebhookSubscription',
              reason: 'The delete returns the proxy-created synthetic webhook subscription ID.',
            },
          ],
        },
      ],
    },
  };
}
