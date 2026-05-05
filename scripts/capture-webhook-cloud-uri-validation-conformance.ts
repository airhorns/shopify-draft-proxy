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
const outputPath = path.join(outputDir, 'webhook-subscription-cloud-uri-validation.json');
const specPath = path.join('config', 'parity-specs', 'webhooks', 'webhook-subscription-cloud-uri-validation.json');

const requestDir = path.join('config', 'parity-requests', 'webhooks');
const createRequestPath = path.join(requestDir, 'webhookSubscriptionCreate-parity.graphql');
const updateRequestPath = path.join(requestDir, 'webhookSubscriptionUpdate-parity.graphql');
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

function extractRequestingApiClientId(captureResult: CapturedRequest): string {
  const serialized = JSON.stringify(captureResult.response.payload);
  const match = serialized.match(/instead of '(\d+)'/u);
  if (!match) {
    throw new Error(`Could not derive requesting api_client_id from ${serialized}`);
  }

  return match[1];
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

const suffix = `${Date.now()}`;
const setupCreateVariables = createVariables(`https://example.com/hermes-cloud-uri-validation-${suffix}`);

const createCases = {
  createPubsubNoTopicRejected: createVariables('pubsub://my-project'),
  createPubsubBadProjectRejected: createVariables('pubsub://-bad:topic'),
  createPubsubBadTopicRejected: createVariables('pubsub://valid-project:goog-prefixed'),
  createArnMalformedRejected: createVariables('arn:aws:events:bogus'),
  createArnWrongApiClientRejected: createVariables(
    'arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/1/source',
  ),
  createKafkaRejected: createVariables('kafka://broker/topic'),
};

let setupCreate: CapturedRequest | null = null;
let cleanup: CapturedRequest | null = null;
let setupId: string | null = null;

try {
  setupCreate = await capture(createRequestPath, setupCreateVariables);
  setupId = readCreatedWebhookId(setupCreate);

  const cases: Record<string, CapturedRequest> = {};
  for (const [name, variables] of Object.entries(createCases)) {
    cases[name] = await capture(createRequestPath, variables);
  }

  const requestingApiClientId = extractRequestingApiClientId(cases['createArnWrongApiClientRejected']);
  const updateCases = {
    updatePubsubNoTopicRejected: updateVariables(setupId, 'pubsub://my-project'),
    updatePubsubBadProjectRejected: updateVariables(setupId, 'pubsub://-bad:topic'),
    updatePubsubBadTopicRejected: updateVariables(setupId, 'pubsub://valid-project:goog-prefixed'),
    updateArnMalformedRejected: updateVariables(setupId, 'arn:aws:events:bogus'),
    updateArnWrongApiClientRejected: updateVariables(
      setupId,
      'arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/1/source',
    ),
    updateKafkaRejected: updateVariables(setupId, 'kafka://broker/topic'),
  };

  for (const [name, variables] of Object.entries(updateCases)) {
    cases[name] = await capture(updateRequestPath, variables);
  }

  cleanup = await capture(deleteRequestPath, { id: setupId });

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        notes: [
          'HAR-724 captures public webhookSubscriptionCreate/update cloud URI validation branches.',
          'The setup webhook uses an example.com HTTP URI and is deleted before the fixture is written.',
          'Current Admin GraphQL UserError has field/message only; MERCHANT_WEBHOOK_ERROR is not exposed as a selectable code field on this schema.',
        ],
        requestingApiClientId,
        upstreamCalls: [],
        setup: {
          create: setupCreate,
          cleanup,
        },
        cases,
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  await writeFile(specPath, `${JSON.stringify(buildSpec(requestingApiClientId), null, 2)}\n`, 'utf8');
} finally {
  if (setupId !== null && cleanup === null) {
    await capture(deleteRequestPath, { id: setupId });
  }
}

console.log(`Wrote webhook cloud URI validation fixture to ${outputPath}`);
console.log(`Wrote webhook cloud URI validation parity spec to ${specPath}`);

function buildSpec(requestingApiClientId: string): Record<string, unknown> {
  const wrongClientHeaders = {
    'x-shopify-draft-proxy-api-client-id': requestingApiClientId,
  };

  return {
    scenarioId: 'webhook-subscription-cloud-uri-validation',
    operationNames: ['webhookSubscriptionCreate', 'webhookSubscriptionUpdate'],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['test/parity_test.gleam', 'test/shopify_draft_proxy/proxy/webhooks_test.gleam'],
    proxyRequest: {
      documentPath: createRequestPath,
      apiVersion,
      variablesCapturePath: '$.setup.create.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Strict parity for public webhook subscription cloud URI validation. The primary request creates a temporary local subscription so update validation can run against an existing proxy ID; comparison targets cover only rejected create/update branches.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        createTarget('create-pubsub-no-topic-rejected', 'createPubsubNoTopicRejected'),
        createTarget('create-pubsub-bad-project-rejected', 'createPubsubBadProjectRejected'),
        createTarget('create-pubsub-bad-topic-rejected', 'createPubsubBadTopicRejected'),
        createTarget('create-arn-malformed-rejected', 'createArnMalformedRejected'),
        createTarget('create-arn-wrong-api-client-rejected', 'createArnWrongApiClientRejected', wrongClientHeaders),
        createTarget('create-kafka-rejected', 'createKafkaRejected'),
        updateTarget('update-pubsub-no-topic-rejected', 'updatePubsubNoTopicRejected'),
        updateTarget('update-pubsub-bad-project-rejected', 'updatePubsubBadProjectRejected'),
        updateTarget('update-pubsub-bad-topic-rejected', 'updatePubsubBadTopicRejected'),
        updateTarget('update-arn-malformed-rejected', 'updateArnMalformedRejected'),
        updateTarget('update-arn-wrong-api-client-rejected', 'updateArnWrongApiClientRejected', wrongClientHeaders),
        updateTarget('update-kafka-rejected', 'updateKafkaRejected'),
      ],
    },
  };
}

function createTarget(name: string, caseName: string, headers?: Record<string, string>): Record<string, unknown> {
  return {
    name,
    capturePath: `$.cases.${caseName}.response.payload.data.webhookSubscriptionCreate`,
    proxyPath: '$.data.webhookSubscriptionCreate',
    proxyRequest: {
      documentPath: createRequestPath,
      variablesCapturePath: `$.cases.${caseName}.variables`,
      ...(headers ? { headers } : {}),
    },
  };
}

function updateTarget(name: string, caseName: string, headers?: Record<string, string>): Record<string, unknown> {
  return {
    name,
    capturePath: `$.cases.${caseName}.response.payload.data.webhookSubscriptionUpdate`,
    proxyPath: '$.data.webhookSubscriptionUpdate',
    proxyRequest: {
      documentPath: updateRequestPath,
      variables: {
        id: {
          fromPrimaryProxyPath: '$.data.webhookSubscriptionCreate.webhookSubscription.id',
        },
        webhookSubscription: {
          fromCapturePath: `$.cases.${caseName}.variables.webhookSubscription`,
        },
      },
      ...(headers ? { headers } : {}),
    },
  };
}
