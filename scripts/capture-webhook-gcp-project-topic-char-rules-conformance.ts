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
const outputPath = path.join(outputDir, 'gcp-project-topic-char-rules.json');
const specPath = path.join('config', 'parity-specs', 'webhooks', 'gcp-project-topic-char-rules.json');

const requestDir = path.join('config', 'parity-requests', 'webhooks');
const dedicatedCreateRequestPath = path.join(requestDir, 'pubSubWebhookSubscriptionCreate-parity.graphql');
const dedicatedUpdateRequestPath = path.join(requestDir, 'pubSubWebhookSubscriptionUpdate-parity.graphql');
const unifiedCreateRequestPath = path.join(requestDir, 'webhookSubscriptionCreate-parity.graphql');
const unifiedUpdateRequestPath = path.join(requestDir, 'webhookSubscriptionUpdate-parity.graphql');
const detailRequestPath = path.join(requestDir, 'webhook-subscription-dedicated-cloud-detail-read.graphql');
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

function requireUserErrors(captureResult: CapturedRequest, rootName: string): void {
  const payload = readPayload(captureResult, rootName);
  const subscription = payload['webhookSubscription'];
  const userErrors = payload['userErrors'];
  if (subscription !== null || !Array.isArray(userErrors) || userErrors.length === 0) {
    throw new Error(`${rootName} did not return the expected blocking userErrors: ${JSON.stringify(payload)}`);
  }
}

function requireNoUserErrors(captureResult: CapturedRequest, rootName: string): void {
  const payload = readPayload(captureResult, rootName);
  const subscription = payload['webhookSubscription'];
  const userErrors = payload['userErrors'];
  if (!isObject(subscription) || !Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${rootName} did not return the expected accepted payload: ${JSON.stringify(payload)}`);
  }
}

function extractRequestingApiClientId(captureResult: CapturedRequest): string {
  const serialized = JSON.stringify(captureResult.response.payload);
  const match = serialized.match(/app--(\d+)/u);
  if (!match?.[1]) {
    throw new Error(`Could not derive requesting api_client_id from ${serialized}`);
  }

  return match[1];
}

function dedicatedCreateVariables(project: string, topic: string): Record<string, unknown> {
  return {
    topic: 'SHOP_UPDATE',
    webhookSubscription: {
      pubSubProject: project,
      pubSubTopic: topic,
      format: 'JSON',
    },
  };
}

function dedicatedUpdateVariables(id: string, project: string, topic: string): Record<string, unknown> {
  return {
    id,
    webhookSubscription: {
      pubSubProject: project,
      pubSubTopic: topic,
      format: 'JSON',
    },
  };
}

function unifiedCreateVariables(uri: string): Record<string, unknown> {
  return {
    topic: 'SHOP_UPDATE',
    webhookSubscription: {
      uri,
      format: 'JSON',
    },
  };
}

function unifiedUpdateVariables(id: string, uri: string): Record<string, unknown> {
  return {
    id,
    webhookSubscription: {
      uri,
      format: 'JSON',
    },
  };
}

const numericProject = '123456789012';
const validProject = 'valid-project';
const validTopic = 'valid-topic';
const dedicatedPercentCreateTopic = 'dedicated%25topic';
const dedicatedPercentUpdateTopic = 'dedicated-next%25topic';
const unifiedPercentCreateTopic = 'unified%25topic';
const unifiedNumericPercentUpdateTopic = 'unified-next%25topic';
const unifiedPercentUpdateTopic = 'unified-final%25topic';
const digitLeadingTopic = '1topic';

const setup: {
  apiClientProbe: CapturedRequest | null;
} = {
  apiClientProbe: null,
};

type GcpCharRuleCases = {
  dedicatedCreateNumericProjectAccepted: CapturedRequest | null;
  dedicatedCreateDigitLeadingTopicRejected: CapturedRequest | null;
  dedicatedCreatePercentTopicAccepted: CapturedRequest | null;
  dedicatedUpdateNumericProjectAccepted: CapturedRequest | null;
  dedicatedUpdateDigitLeadingTopicRejected: CapturedRequest | null;
  dedicatedUpdatePercentTopicAccepted: CapturedRequest | null;
  dedicatedDetailAfterNumericProjectUpdate: CapturedRequest | null;
  unifiedCreateNumericProjectAccepted: CapturedRequest | null;
  unifiedCreateDigitLeadingTopicRejected: CapturedRequest | null;
  unifiedCreatePercentTopicAccepted: CapturedRequest | null;
  unifiedUpdateNumericProjectAccepted: CapturedRequest | null;
  unifiedUpdateDigitLeadingTopicRejected: CapturedRequest | null;
  unifiedUpdatePercentTopicAccepted: CapturedRequest | null;
  unifiedDetailAfterPercentTopicUpdate: CapturedRequest | null;
};

const cases: GcpCharRuleCases = {
  dedicatedCreateNumericProjectAccepted: null,
  dedicatedCreateDigitLeadingTopicRejected: null,
  dedicatedCreatePercentTopicAccepted: null,
  dedicatedUpdateNumericProjectAccepted: null,
  dedicatedUpdateDigitLeadingTopicRejected: null,
  dedicatedUpdatePercentTopicAccepted: null,
  dedicatedDetailAfterNumericProjectUpdate: null,
  unifiedCreateNumericProjectAccepted: null,
  unifiedCreateDigitLeadingTopicRejected: null,
  unifiedCreatePercentTopicAccepted: null,
  unifiedUpdateNumericProjectAccepted: null,
  unifiedUpdateDigitLeadingTopicRejected: null,
  unifiedUpdatePercentTopicAccepted: null,
  unifiedDetailAfterPercentTopicUpdate: null,
};

const cleanup: Record<string, CapturedRequest | null> = {};
const createdIds: string[] = [];

async function cleanupCreatedIds(): Promise<void> {
  for (const id of createdIds.splice(0).reverse()) {
    if (cleanup[id] !== undefined) {
      continue;
    }
    cleanup[id] = await capture(deleteRequestPath, { id });
  }
}

try {
  setup.apiClientProbe = await capture(unifiedCreateRequestPath, {
    topic: 'SHOP_UPDATE',
    webhookSubscription: {
      uri: `https://example.com/hermes-gcp-char-rules-probe-${Date.now()}`,
      format: 'JSON',
      metafieldNamespaces: ['$app:gcpCharRulesProbe'],
    },
  });
  requireNoUserErrors(setup.apiClientProbe, 'webhookSubscriptionCreate');
  const probeId = readCreatedWebhookId(setup.apiClientProbe, 'webhookSubscriptionCreate');
  createdIds.push(probeId);
  const requestingApiClientId = extractRequestingApiClientId(setup.apiClientProbe);

  cases.dedicatedCreateNumericProjectAccepted = await capture(
    dedicatedCreateRequestPath,
    dedicatedCreateVariables(numericProject, validTopic),
  );
  requireNoUserErrors(cases.dedicatedCreateNumericProjectAccepted, 'pubSubWebhookSubscriptionCreate');
  const dedicatedNumericId = readCreatedWebhookId(
    cases.dedicatedCreateNumericProjectAccepted,
    'pubSubWebhookSubscriptionCreate',
  );
  createdIds.push(dedicatedNumericId);

  cases.dedicatedCreateDigitLeadingTopicRejected = await capture(
    dedicatedCreateRequestPath,
    dedicatedCreateVariables(validProject, digitLeadingTopic),
  );
  requireUserErrors(cases.dedicatedCreateDigitLeadingTopicRejected, 'pubSubWebhookSubscriptionCreate');

  cases.dedicatedCreatePercentTopicAccepted = await capture(
    dedicatedCreateRequestPath,
    dedicatedCreateVariables(validProject, dedicatedPercentCreateTopic),
  );
  requireNoUserErrors(cases.dedicatedCreatePercentTopicAccepted, 'pubSubWebhookSubscriptionCreate');
  const dedicatedPercentId = readCreatedWebhookId(
    cases.dedicatedCreatePercentTopicAccepted,
    'pubSubWebhookSubscriptionCreate',
  );
  createdIds.push(dedicatedPercentId);

  cases.dedicatedUpdateNumericProjectAccepted = await capture(
    dedicatedUpdateRequestPath,
    dedicatedUpdateVariables(dedicatedNumericId, numericProject, dedicatedPercentUpdateTopic),
  );
  requireNoUserErrors(cases.dedicatedUpdateNumericProjectAccepted, 'pubSubWebhookSubscriptionUpdate');

  cases.dedicatedUpdateDigitLeadingTopicRejected = await capture(
    dedicatedUpdateRequestPath,
    dedicatedUpdateVariables(dedicatedNumericId, validProject, digitLeadingTopic),
  );
  requireUserErrors(cases.dedicatedUpdateDigitLeadingTopicRejected, 'pubSubWebhookSubscriptionUpdate');

  cases.dedicatedUpdatePercentTopicAccepted = await capture(
    dedicatedUpdateRequestPath,
    dedicatedUpdateVariables(dedicatedNumericId, numericProject, dedicatedPercentCreateTopic),
  );
  requireNoUserErrors(cases.dedicatedUpdatePercentTopicAccepted, 'pubSubWebhookSubscriptionUpdate');

  cases.dedicatedDetailAfterNumericProjectUpdate = await capture(detailRequestPath, { id: dedicatedNumericId });

  cases.unifiedCreateNumericProjectAccepted = await capture(
    unifiedCreateRequestPath,
    unifiedCreateVariables(`pubsub://${numericProject}:${validTopic}`),
  );
  requireNoUserErrors(cases.unifiedCreateNumericProjectAccepted, 'webhookSubscriptionCreate');
  const unifiedNumericId = readCreatedWebhookId(cases.unifiedCreateNumericProjectAccepted, 'webhookSubscriptionCreate');
  createdIds.push(unifiedNumericId);

  cases.unifiedCreateDigitLeadingTopicRejected = await capture(
    unifiedCreateRequestPath,
    unifiedCreateVariables(`pubsub://${validProject}:${digitLeadingTopic}`),
  );
  requireUserErrors(cases.unifiedCreateDigitLeadingTopicRejected, 'webhookSubscriptionCreate');

  cases.unifiedCreatePercentTopicAccepted = await capture(
    unifiedCreateRequestPath,
    unifiedCreateVariables(`pubsub://${validProject}:${unifiedPercentCreateTopic}`),
  );
  requireNoUserErrors(cases.unifiedCreatePercentTopicAccepted, 'webhookSubscriptionCreate');
  const unifiedPercentId = readCreatedWebhookId(cases.unifiedCreatePercentTopicAccepted, 'webhookSubscriptionCreate');
  createdIds.push(unifiedPercentId);

  cases.unifiedUpdateNumericProjectAccepted = await capture(
    unifiedUpdateRequestPath,
    unifiedUpdateVariables(unifiedNumericId, `pubsub://${numericProject}:${unifiedNumericPercentUpdateTopic}`),
  );
  requireNoUserErrors(cases.unifiedUpdateNumericProjectAccepted, 'webhookSubscriptionUpdate');

  cases.unifiedUpdateDigitLeadingTopicRejected = await capture(
    unifiedUpdateRequestPath,
    unifiedUpdateVariables(unifiedNumericId, `pubsub://${validProject}:${digitLeadingTopic}`),
  );
  requireUserErrors(cases.unifiedUpdateDigitLeadingTopicRejected, 'webhookSubscriptionUpdate');

  cases.unifiedUpdatePercentTopicAccepted = await capture(
    unifiedUpdateRequestPath,
    unifiedUpdateVariables(unifiedPercentId, `pubsub://${numericProject}:${unifiedPercentUpdateTopic}`),
  );
  requireNoUserErrors(cases.unifiedUpdatePercentTopicAccepted, 'webhookSubscriptionUpdate');

  cases.unifiedDetailAfterPercentTopicUpdate = await capture(detailRequestPath, { id: unifiedPercentId });

  await cleanupCreatedIds();

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
          'Captures Shopify GCP Pub/Sub project/topic character validation for dedicated Pub/Sub and unified webhookSubscription roots.',
          'Project numbers made only of digits are accepted in addition to project IDs that follow the lowercase alpha-start rule.',
          'Topic IDs must start with an ASCII letter, reject digit-leading topics, and allow percent signs in the topic charset.',
          'Temporary webhook subscriptions are deleted during cleanup and the script does not trigger webhook delivery.',
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

  await writeFile(specPath, `${JSON.stringify(buildSpec(requestingApiClientId), null, 2)}\n`, 'utf8');
} finally {
  await cleanupCreatedIds();
}

console.log(`Wrote GCP project/topic char-rules fixture to ${outputPath}`);
console.log(`Wrote GCP project/topic char-rules parity spec to ${specPath}`);

function buildSpec(apiClientId: string): Record<string, unknown> {
  const headers = {
    'x-shopify-draft-proxy-api-client-id': apiClientId,
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
  const payloadDifferences = [idDifference, createdAtDifference, updatedAtDifference];
  const detailDifferences = [
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
  ];
  const dedicatedNumericIdRef = {
    fromPrimaryProxyPath: '$.data.pubSubWebhookSubscriptionCreate.webhookSubscription.id',
  };
  const unifiedNumericIdRef = {
    fromProxyResponse: 'unified-create-numeric-project-accepted',
    path: '$.data.webhookSubscriptionCreate.webhookSubscription.id',
  };
  const unifiedPercentIdRef = {
    fromProxyResponse: 'unified-create-percent-topic-accepted',
    path: '$.data.webhookSubscriptionCreate.webhookSubscription.id',
  };

  return {
    scenarioId: 'gcp-project-topic-char-rules',
    operationNames: [
      'pubSubWebhookSubscriptionCreate',
      'pubSubWebhookSubscriptionUpdate',
      'webhookSubscriptionCreate',
      'webhookSubscriptionUpdate',
      'webhookSubscription',
    ],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'user-errors-parity', 'read-after-write', 'downstream-read-parity'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['tests/graphql_routes/admin_graphql_webhooks.rs'],
    proxyRequest: {
      documentPath: dedicatedCreateRequestPath,
      variablesCapturePath: '$.cases.dedicatedCreateNumericProjectAccepted.variables',
      apiVersion,
      headers,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Strict parity for GCP Pub/Sub project/topic character rules on dedicated Pub/Sub and unified webhookSubscription roots. The replay proves numeric project-number acceptance, digit-leading topic rejection, percent-topic acceptance, update behavior, and downstream detail readback without runtime Shopify writes.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        acceptedTarget(
          'dedicated-create-numeric-project-accepted',
          '$.cases.dedicatedCreateNumericProjectAccepted.response.payload.data.pubSubWebhookSubscriptionCreate',
          '$.data.pubSubWebhookSubscriptionCreate',
          undefined,
          payloadDifferences,
        ),
        createTarget(
          'dedicated-create-digit-leading-topic-rejected',
          dedicatedCreateRequestPath,
          '$.cases.dedicatedCreateDigitLeadingTopicRejected',
          '$.data.pubSubWebhookSubscriptionCreate',
          headers,
        ),
        createTarget(
          'dedicated-create-percent-topic-accepted',
          dedicatedCreateRequestPath,
          '$.cases.dedicatedCreatePercentTopicAccepted',
          '$.data.pubSubWebhookSubscriptionCreate',
          headers,
          payloadDifferences,
        ),
        updateTarget(
          'dedicated-update-numeric-project-accepted',
          dedicatedUpdateRequestPath,
          '$.cases.dedicatedUpdateNumericProjectAccepted',
          '$.data.pubSubWebhookSubscriptionUpdate',
          dedicatedNumericIdRef,
          headers,
          payloadDifferences,
        ),
        updateTarget(
          'dedicated-update-digit-leading-topic-rejected',
          dedicatedUpdateRequestPath,
          '$.cases.dedicatedUpdateDigitLeadingTopicRejected',
          '$.data.pubSubWebhookSubscriptionUpdate',
          dedicatedNumericIdRef,
          headers,
        ),
        updateTarget(
          'dedicated-update-percent-topic-accepted',
          dedicatedUpdateRequestPath,
          '$.cases.dedicatedUpdatePercentTopicAccepted',
          '$.data.pubSubWebhookSubscriptionUpdate',
          dedicatedNumericIdRef,
          headers,
          payloadDifferences,
        ),
        detailTarget(
          'dedicated-detail-after-numeric-project-update',
          '$.cases.dedicatedDetailAfterNumericProjectUpdate.response.payload.data.webhookSubscription',
          dedicatedNumericIdRef,
          headers,
          detailDifferences,
        ),
        createTarget(
          'unified-create-numeric-project-accepted',
          unifiedCreateRequestPath,
          '$.cases.unifiedCreateNumericProjectAccepted',
          '$.data.webhookSubscriptionCreate',
          headers,
          payloadDifferences,
        ),
        createTarget(
          'unified-create-digit-leading-topic-rejected',
          unifiedCreateRequestPath,
          '$.cases.unifiedCreateDigitLeadingTopicRejected',
          '$.data.webhookSubscriptionCreate',
          headers,
        ),
        createTarget(
          'unified-create-percent-topic-accepted',
          unifiedCreateRequestPath,
          '$.cases.unifiedCreatePercentTopicAccepted',
          '$.data.webhookSubscriptionCreate',
          headers,
          payloadDifferences,
        ),
        updateTarget(
          'unified-update-numeric-project-accepted',
          unifiedUpdateRequestPath,
          '$.cases.unifiedUpdateNumericProjectAccepted',
          '$.data.webhookSubscriptionUpdate',
          unifiedNumericIdRef,
          headers,
          payloadDifferences,
        ),
        updateTarget(
          'unified-update-digit-leading-topic-rejected',
          unifiedUpdateRequestPath,
          '$.cases.unifiedUpdateDigitLeadingTopicRejected',
          '$.data.webhookSubscriptionUpdate',
          unifiedNumericIdRef,
          headers,
        ),
        updateTarget(
          'unified-update-percent-topic-accepted',
          unifiedUpdateRequestPath,
          '$.cases.unifiedUpdatePercentTopicAccepted',
          '$.data.webhookSubscriptionUpdate',
          unifiedPercentIdRef,
          headers,
          payloadDifferences,
        ),
        detailTarget(
          'unified-detail-after-percent-topic-update',
          '$.cases.unifiedDetailAfterPercentTopicUpdate.response.payload.data.webhookSubscription',
          unifiedPercentIdRef,
          headers,
          detailDifferences,
        ),
      ],
    },
  };
}

function acceptedTarget(
  name: string,
  capturePath: string,
  proxyPath: string,
  proxyRequest: Record<string, unknown> | undefined,
  expectedDifferences: Array<Record<string, unknown>>,
): Record<string, unknown> {
  return {
    name,
    capturePath,
    proxyPath,
    ...(proxyRequest ? { proxyRequest } : {}),
    expectedDifferences,
  };
}

function createTarget(
  name: string,
  documentPath: string,
  casePath: string,
  proxyPath: string,
  headers: Record<string, string>,
  expectedDifferences?: Array<Record<string, unknown>>,
): Record<string, unknown> {
  return acceptedTarget(
    name,
    `${casePath}.response.payload.data.${rootNameFromProxyPath(proxyPath)}`,
    proxyPath,
    {
      documentPath,
      variablesCapturePath: `${casePath}.variables`,
      apiVersion,
      headers,
    },
    expectedDifferences ?? [],
  );
}

function updateTarget(
  name: string,
  documentPath: string,
  casePath: string,
  proxyPath: string,
  idRef: Record<string, string>,
  headers: Record<string, string>,
  expectedDifferences?: Array<Record<string, unknown>>,
): Record<string, unknown> {
  return acceptedTarget(
    name,
    `${casePath}.response.payload.data.${rootNameFromProxyPath(proxyPath)}`,
    proxyPath,
    {
      documentPath,
      variables: {
        id: idRef,
        webhookSubscription: {
          fromCapturePath: `${casePath}.variables.webhookSubscription`,
        },
      },
      apiVersion,
      headers,
    },
    expectedDifferences ?? [],
  );
}

function detailTarget(
  name: string,
  capturePath: string,
  idRef: Record<string, string>,
  headers: Record<string, string>,
  expectedDifferences: Array<Record<string, unknown>>,
): Record<string, unknown> {
  return {
    name,
    capturePath,
    proxyPath: '$.data.webhookSubscription',
    proxyRequest: {
      documentPath: detailRequestPath,
      variables: {
        id: idRef,
      },
      apiVersion,
      headers,
    },
    expectedDifferences,
  };
}

function rootNameFromProxyPath(proxyPath: string): string {
  const rootName = proxyPath.split('.').at(-1);
  if (typeof rootName !== 'string' || rootName.length === 0) {
    throw new Error(`Could not derive root name from proxy path ${proxyPath}`);
  }
  return rootName;
}
