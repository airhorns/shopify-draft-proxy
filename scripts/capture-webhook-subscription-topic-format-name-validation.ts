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
const outputPath = path.join(outputDir, 'webhook-subscription-topic-format-name-validation.json');

const requestDir = path.join('config', 'parity-requests', 'webhooks');
const createRequestPath = path.join(requestDir, 'webhook-subscription-topic-format-name-create.graphql');
const updateRequestPath = path.join(requestDir, 'webhook-subscription-topic-format-name-update.graphql');
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

  return {
    documentPath,
    variables,
    response,
  };
}

function readCreatedWebhookId(createCapture: CapturedRequest): string | null {
  const data = createCapture.response.payload.data;
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

const suffix = `${Date.now()}`;
const duplicateUri = `https://example.com/hermes-webhook-topic-format-name-${suffix}`;
const validation = {
  createXmlOnJsonOnlyTopicRejected: null as CapturedRequest | null,
  createXmlOnCloudDeliveryRejected: null as CapturedRequest | null,
  createEmptyNameRejected: null as CapturedRequest | null,
  createBadNameFormatRejected: null as CapturedRequest | null,
  createDuplicateSetup: null as CapturedRequest | null,
  createDuplicateRejected: null as CapturedRequest | null,
  updateBadNameFormatRejected: null as CapturedRequest | null,
  cleanup: null as CapturedRequest | null,
};
let createdId: string | null = null;

try {
  validation.createXmlOnJsonOnlyTopicRejected = await capture(createRequestPath, {
    topic: 'RETURNS_APPROVE',
    webhookSubscription: {
      uri: `${duplicateUri}-returns-xml`,
      format: 'XML',
    },
  });
  validation.createXmlOnCloudDeliveryRejected = await capture(createRequestPath, {
    topic: 'SHOP_UPDATE',
    webhookSubscription: {
      uri: 'pubsub://valid-project:topic',
      format: 'XML',
    },
  });
  validation.createEmptyNameRejected = await capture(createRequestPath, {
    topic: 'SHOP_UPDATE',
    webhookSubscription: {
      uri: `${duplicateUri}-empty-name`,
      name: '',
    },
  });
  validation.createBadNameFormatRejected = await capture(createRequestPath, {
    topic: 'SHOP_UPDATE',
    webhookSubscription: {
      uri: `${duplicateUri}-bad-name`,
      name: 'has spaces',
    },
  });
  validation.createDuplicateSetup = await capture(createRequestPath, {
    topic: 'SHOP_UPDATE',
    webhookSubscription: {
      uri: duplicateUri,
      format: 'JSON',
      filter: '',
    },
  });
  createdId = readCreatedWebhookId(validation.createDuplicateSetup);
  if (createdId === null) {
    throw new Error('duplicate setup did not return a webhookSubscription.id.');
  }

  validation.createDuplicateRejected = await capture(createRequestPath, {
    topic: 'SHOP_UPDATE',
    webhookSubscription: {
      uri: duplicateUri,
      format: 'JSON',
      filter: '',
    },
  });
  validation.updateBadNameFormatRejected = await capture(updateRequestPath, {
    id: createdId,
    webhookSubscription: {
      uri: duplicateUri,
      name: 'has spaces',
    },
  });
} finally {
  if (createdId !== null) {
    validation.cleanup = await capture(deleteRequestPath, { id: createdId });
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
        'HAR-727 captures webhookSubscriptionCreate topic/format, cloud format, name validation, and duplicate active registration userErrors.',
        'The duplicate branch creates one temporary SHOP_UPDATE HTTP subscription and deletes it during cleanup.',
        'The script does not trigger webhook delivery.',
      ],
      deliveryPolicy: {
        deliveriesTriggeredByScript: false,
        topicUsedForDuplicateSetup: 'SHOP_UPDATE',
        endpointHost: 'example.com',
      },
      validation,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote webhook subscription validation fixture to ${outputPath}`);
