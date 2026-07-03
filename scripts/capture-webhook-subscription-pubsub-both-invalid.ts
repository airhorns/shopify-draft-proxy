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
const outputPath = path.join(outputDir, 'webhook-subscription-pubsub-both-invalid.json');
const specPath = path.join('config', 'parity-specs', 'webhooks', 'webhook-subscription-pubsub-both-invalid.json');

const requestDir = path.join('config', 'parity-requests', 'webhooks');
const createRequestPath = path.join(requestDir, 'pubSubWebhookSubscriptionCreate-parity.graphql');
const updateRequestPath = path.join(requestDir, 'pubSubWebhookSubscriptionUpdate-parity.graphql');
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

function readPayload(captureResult: CapturedRequest, rootName: string): Record<string, unknown> {
  const data = captureResult.response.payload.data;
  if (!isObject(data)) {
    throw new Error(`${rootName} response did not include data.`);
  }

  const payload = data[rootName];
  if (!isObject(payload)) {
    throw new Error(`${rootName} payload is missing.`);
  }

  return payload;
}

function readCreatedWebhookId(captureResult: CapturedRequest, rootName: string): string {
  const payload = readPayload(captureResult, rootName);
  const subscription = payload['webhookSubscription'];
  if (!isObject(subscription) || typeof subscription['id'] !== 'string') {
    throw new Error(`${rootName} did not return a webhookSubscription.id.`);
  }

  return subscription['id'];
}

const expectedBothInvalidUserErrors = [
  {
    field: ['webhookSubscription', 'pubSubProject'],
    message: 'Google Cloud Pub/Sub project ID is not valid',
  },
  {
    field: ['webhookSubscription', 'pubSubTopic'],
    message: 'Google Cloud Pub/Sub topic ID is not valid',
  },
];

function requireNoUserErrors(captureResult: CapturedRequest, rootName: string): void {
  const payload = readPayload(captureResult, rootName);
  const subscription = payload['webhookSubscription'];
  const userErrors = payload['userErrors'];
  if (!isObject(subscription) || !Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${rootName} did not return the expected accepted payload: ${JSON.stringify(payload)}`);
  }
}

function requireBothInvalidUserErrors(captureResult: CapturedRequest, rootName: string): void {
  const payload = readPayload(captureResult, rootName);
  const subscription = payload['webhookSubscription'];
  const userErrors = payload['userErrors'];
  if (subscription !== null || JSON.stringify(userErrors) !== JSON.stringify(expectedBothInvalidUserErrors)) {
    throw new Error(`${rootName} did not return both Pub/Sub field userErrors: ${JSON.stringify(payload)}`);
  }
}

function createVariables(project: string, topic: string): Record<string, unknown> {
  return {
    topic: 'SHOP_UPDATE',
    webhookSubscription: {
      pubSubProject: project,
      pubSubTopic: topic,
      format: 'JSON',
    },
  };
}

function updateVariables(id: string, project: string, topic: string): Record<string, unknown> {
  return {
    id,
    webhookSubscription: {
      pubSubProject: project,
      pubSubTopic: topic,
      format: 'JSON',
    },
  };
}

const runId = Date.now().toString(36);
const validProject = 'valid-project';
const baselineTopic = `topic-${runId}`;
const invalidProject = '-bad';
const invalidTopic = '1topic';

const setup: {
  baselineCreate: CapturedRequest | null;
} = {
  baselineCreate: null,
};
const cases: {
  dedicatedCreateBothInvalid: CapturedRequest | null;
  dedicatedUpdateBothInvalid: CapturedRequest | null;
} = {
  dedicatedCreateBothInvalid: null,
  dedicatedUpdateBothInvalid: null,
};
const cleanup: {
  baselineDelete: CapturedRequest | null;
} = {
  baselineDelete: null,
};

let baselineId: string | null = null;

try {
  cases.dedicatedCreateBothInvalid = await capture(createRequestPath, createVariables(invalidProject, invalidTopic));
  requireBothInvalidUserErrors(cases.dedicatedCreateBothInvalid, 'pubSubWebhookSubscriptionCreate');

  setup.baselineCreate = await capture(createRequestPath, createVariables(validProject, baselineTopic));
  requireNoUserErrors(setup.baselineCreate, 'pubSubWebhookSubscriptionCreate');
  baselineId = readCreatedWebhookId(setup.baselineCreate, 'pubSubWebhookSubscriptionCreate');

  cases.dedicatedUpdateBothInvalid = await capture(
    updateRequestPath,
    updateVariables(baselineId, invalidProject, invalidTopic),
  );
  requireBothInvalidUserErrors(cases.dedicatedUpdateBothInvalid, 'pubSubWebhookSubscriptionUpdate');

  cleanup.baselineDelete = await capture(deleteRequestPath, { id: baselineId });

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        notes: [
          'Captures Shopify userErrors when both pubSubProject and pubSubTopic are invalid on the deprecated dedicated Pub/Sub webhook subscription create/update roots.',
          'The project and topic validators accumulate independently on the dedicated roots; project userError is emitted before topic userError.',
          'A temporary valid Pub/Sub subscription is created only so the update branch targets a real Shopify ID, then deleted during cleanup.',
        ],
        setup,
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
} finally {
  if (baselineId !== null && cleanup.baselineDelete === null) {
    await capture(deleteRequestPath, { id: baselineId });
  }
}

console.log(`Wrote Pub/Sub both-invalid webhook fixture to ${outputPath}`);
console.log(`Wrote Pub/Sub both-invalid webhook parity spec to ${specPath}`);

function buildSpec(): Record<string, unknown> {
  const baselineIdRef = {
    fromPrimaryProxyPath: '$.data.pubSubWebhookSubscriptionCreate.webhookSubscription.id',
  };

  return {
    scenarioId: 'webhook-subscription-pubsub-both-invalid',
    operationNames: ['pubSubWebhookSubscriptionCreate', 'pubSubWebhookSubscriptionUpdate'],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity', 'payload-shape', 'no-local-staging-on-validation-error'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['tests/graphql_routes/admin_graphql_webhooks.rs'],
    proxyRequest: {
      documentPath: createRequestPath,
      variablesCapturePath: '$.setup.baselineCreate.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Strict parity for dedicated Pub/Sub webhook subscription create/update when pubSubProject and pubSubTopic are both invalid. The primary request stages a valid baseline through public GraphQL so the update target can replay against the proxy-created ID; rejected targets assert the complete Shopify userErrors array in order.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'dedicated-create-both-invalid',
          capturePath: '$.cases.dedicatedCreateBothInvalid.response.payload.data.pubSubWebhookSubscriptionCreate',
          proxyPath: '$.data.pubSubWebhookSubscriptionCreate',
          proxyRequest: {
            documentPath: createRequestPath,
            variablesCapturePath: '$.cases.dedicatedCreateBothInvalid.variables',
            apiVersion,
          },
          expectedDifferences: [],
        },
        {
          name: 'dedicated-update-both-invalid',
          capturePath: '$.cases.dedicatedUpdateBothInvalid.response.payload.data.pubSubWebhookSubscriptionUpdate',
          proxyPath: '$.data.pubSubWebhookSubscriptionUpdate',
          proxyRequest: {
            documentPath: updateRequestPath,
            variables: {
              id: baselineIdRef,
              webhookSubscription: {
                fromCapturePath: '$.cases.dedicatedUpdateBothInvalid.variables.webhookSubscription',
              },
            },
            apiVersion,
          },
          expectedDifferences: [],
        },
      ],
    },
  };
}
