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
const outputPath = path.join(outputDir, 'webhook-subscription-dedicated-cloud-destinations.json');
const specPath = path.join(
  'config',
  'parity-specs',
  'webhooks',
  'webhook-subscription-dedicated-cloud-destinations.json',
);

const requestDir = path.join('config', 'parity-requests', 'webhooks');
const pubSubCreateRequestPath = path.join(requestDir, 'pubSubWebhookSubscriptionCreate-parity.graphql');
const pubSubUpdateRequestPath = path.join(requestDir, 'pubSubWebhookSubscriptionUpdate-parity.graphql');
const eventBridgeCreateRequestPath = path.join(requestDir, 'eventBridgeWebhookSubscriptionCreate-parity.graphql');
const eventBridgeUpdateRequestPath = path.join(requestDir, 'eventBridgeWebhookSubscriptionUpdate-parity.graphql');
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
  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length === 0) {
    throw new Error(`${rootName} did not return the expected userErrors: ${JSON.stringify(payload)}`);
  }
}

function extractRequestingApiClientId(captureResult: CapturedRequest): string {
  const serialized = JSON.stringify(captureResult.response.payload);
  const match = serialized.match(/instead of '(\d+)'/u);
  if (!match?.[1]) {
    throw new Error(`Could not derive requesting api_client_id from ${serialized}`);
  }

  return match[1];
}

function pubSubCreateVariables(project: string, topic: string): Record<string, unknown> {
  return {
    topic: 'SHOP_UPDATE',
    webhookSubscription: {
      pubSubProject: project,
      pubSubTopic: topic,
      format: 'JSON',
    },
  };
}

function pubSubUpdateVariables(id: string, project: string, topic: string): Record<string, unknown> {
  return {
    id,
    webhookSubscription: {
      pubSubProject: project,
      pubSubTopic: topic,
      format: 'JSON',
    },
  };
}

function eventBridgeCreateVariables(arn: string): Record<string, unknown> {
  return {
    topic: 'SHOP_UPDATE',
    webhookSubscription: {
      arn,
      format: 'JSON',
    },
  };
}

function eventBridgeUpdateVariables(id: string, arn: string): Record<string, unknown> {
  return {
    id,
    webhookSubscription: {
      arn,
      format: 'JSON',
    },
  };
}

function eventBridgeArn(region: string, apiClientId: string, sourceName: string): string {
  return `arn:aws:events:${region}::event-source/aws.partner/shopify.com/${apiClientId}/${sourceName}`;
}

const runId = Date.now().toString(36);
const pubSubProject = 'valid-project';
const pubSubCreateTopic = `topic-${runId}`;
const pubSubUpdateTopic = `topic-${runId}-updated`;
const wrongEventBridgeArn = eventBridgeArn('us-east-1', '1', `source-${runId}`);

const validation: {
  createPubSubBadProject: CapturedRequest | null;
  createPubSubBadTopic: CapturedRequest | null;
  updatePubSubBadProject: CapturedRequest | null;
  updatePubSubBadTopic: CapturedRequest | null;
  createEventBridgeMalformedArn: CapturedRequest | null;
  createEventBridgeWrongApiClient: CapturedRequest | null;
  updateEventBridgeMalformedArn: CapturedRequest | null;
  updateEventBridgeWrongApiClient: CapturedRequest | null;
} = {
  createPubSubBadProject: null,
  createPubSubBadTopic: null,
  updatePubSubBadProject: null,
  updatePubSubBadTopic: null,
  createEventBridgeMalformedArn: null,
  createEventBridgeWrongApiClient: null,
  updateEventBridgeMalformedArn: null,
  updateEventBridgeWrongApiClient: null,
};

const pubSubLifecycle: {
  create: CapturedRequest | null;
  detailAfterCreate: CapturedRequest | null;
  update: CapturedRequest | null;
  detailAfterUpdate: CapturedRequest | null;
  delete: CapturedRequest | null;
} = {
  create: null,
  detailAfterCreate: null,
  update: null,
  detailAfterUpdate: null,
  delete: null,
};

const eventBridgeLifecycle: {
  create: CapturedRequest | null;
  detailAfterCreate: CapturedRequest | null;
  update: CapturedRequest | null;
  detailAfterUpdate: CapturedRequest | null;
  delete: CapturedRequest | null;
} = {
  create: null,
  detailAfterCreate: null,
  update: null,
  detailAfterUpdate: null,
  delete: null,
};

let requestingApiClientId: string | null = null;
let pubSubId: string | null = null;
let eventBridgeId: string | null = null;

try {
  validation.createEventBridgeWrongApiClient = await capture(
    eventBridgeCreateRequestPath,
    eventBridgeCreateVariables(wrongEventBridgeArn),
  );
  requireUserErrors(validation.createEventBridgeWrongApiClient, 'eventBridgeWebhookSubscriptionCreate');
  requestingApiClientId = extractRequestingApiClientId(validation.createEventBridgeWrongApiClient);

  const validEventBridgeCreateArn = eventBridgeArn('us-east-1', requestingApiClientId, `source-${runId}`);
  const validEventBridgeUpdateArn = eventBridgeArn('us-west-2', requestingApiClientId, `source-${runId}-updated`);
  const malformedEventBridgeArn = `arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/${requestingApiClientId}`;

  validation.createPubSubBadProject = await capture(
    pubSubCreateRequestPath,
    pubSubCreateVariables('-bad-project', `topic-${runId}-invalid-project`),
  );
  requireUserErrors(validation.createPubSubBadProject, 'pubSubWebhookSubscriptionCreate');
  validation.createPubSubBadTopic = await capture(
    pubSubCreateRequestPath,
    pubSubCreateVariables(pubSubProject, 'goog-prefixed-topic'),
  );
  requireUserErrors(validation.createPubSubBadTopic, 'pubSubWebhookSubscriptionCreate');
  validation.createEventBridgeMalformedArn = await capture(
    eventBridgeCreateRequestPath,
    eventBridgeCreateVariables(malformedEventBridgeArn),
  );
  requireUserErrors(validation.createEventBridgeMalformedArn, 'eventBridgeWebhookSubscriptionCreate');

  pubSubLifecycle.create = await capture(
    pubSubCreateRequestPath,
    pubSubCreateVariables(pubSubProject, pubSubCreateTopic),
  );
  pubSubId = readCreatedWebhookId(pubSubLifecycle.create, 'pubSubWebhookSubscriptionCreate');
  pubSubLifecycle.detailAfterCreate = await capture(detailRequestPath, { id: pubSubId });
  validation.updatePubSubBadProject = await capture(
    pubSubUpdateRequestPath,
    pubSubUpdateVariables(pubSubId, '-bad-project', `topic-${runId}-bad-update-project`),
  );
  requireUserErrors(validation.updatePubSubBadProject, 'pubSubWebhookSubscriptionUpdate');
  validation.updatePubSubBadTopic = await capture(
    pubSubUpdateRequestPath,
    pubSubUpdateVariables(pubSubId, pubSubProject, 'goog-prefixed-topic'),
  );
  requireUserErrors(validation.updatePubSubBadTopic, 'pubSubWebhookSubscriptionUpdate');
  pubSubLifecycle.update = await capture(
    pubSubUpdateRequestPath,
    pubSubUpdateVariables(pubSubId, pubSubProject, pubSubUpdateTopic),
  );
  pubSubLifecycle.detailAfterUpdate = await capture(detailRequestPath, { id: pubSubId });

  eventBridgeLifecycle.create = await capture(
    eventBridgeCreateRequestPath,
    eventBridgeCreateVariables(validEventBridgeCreateArn),
  );
  eventBridgeId = readCreatedWebhookId(eventBridgeLifecycle.create, 'eventBridgeWebhookSubscriptionCreate');
  eventBridgeLifecycle.detailAfterCreate = await capture(detailRequestPath, { id: eventBridgeId });
  validation.updateEventBridgeMalformedArn = await capture(
    eventBridgeUpdateRequestPath,
    eventBridgeUpdateVariables(eventBridgeId, malformedEventBridgeArn),
  );
  requireUserErrors(validation.updateEventBridgeMalformedArn, 'eventBridgeWebhookSubscriptionUpdate');
  validation.updateEventBridgeWrongApiClient = await capture(
    eventBridgeUpdateRequestPath,
    eventBridgeUpdateVariables(eventBridgeId, wrongEventBridgeArn),
  );
  requireUserErrors(validation.updateEventBridgeWrongApiClient, 'eventBridgeWebhookSubscriptionUpdate');
  eventBridgeLifecycle.update = await capture(
    eventBridgeUpdateRequestPath,
    eventBridgeUpdateVariables(eventBridgeId, validEventBridgeUpdateArn),
  );
  eventBridgeLifecycle.detailAfterUpdate = await capture(detailRequestPath, { id: eventBridgeId });

  pubSubLifecycle.delete = await capture(deleteRequestPath, { id: pubSubId });
  eventBridgeLifecycle.delete = await capture(deleteRequestPath, { id: eventBridgeId });

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
          'Captures the deprecated dedicated Pub/Sub and EventBridge webhook subscription create/update roots against Shopify Admin GraphQL.',
          'The dedicated Pub/Sub input stores the equivalent pubsub://project:topic URI and projects WebhookPubSubEndpoint on downstream reads.',
          'The dedicated EventBridge input stores the ARN as the equivalent URI and projects WebhookEventBridgeEndpoint on downstream reads.',
          'Top-level deprecated callbackUrl returns the Shopify cloud placeholder while uri and endpoint expose the real cloud destination.',
          'Validation cases capture dedicated input field paths for Pub/Sub project/topic and EventBridge ARN failures.',
          'Temporary subscriptions are deleted during cleanup and the script does not trigger webhook delivery.',
        ],
        validation,
        pubSubLifecycle,
        eventBridgeLifecycle,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  await writeFile(specPath, `${JSON.stringify(buildSpec(requestingApiClientId), null, 2)}\n`, 'utf8');
} finally {
  if (pubSubId !== null && pubSubLifecycle.delete === null) {
    await capture(deleteRequestPath, { id: pubSubId });
  }
  if (eventBridgeId !== null && eventBridgeLifecycle.delete === null) {
    await capture(deleteRequestPath, { id: eventBridgeId });
  }
}

console.log(`Wrote dedicated cloud webhook fixture to ${outputPath}`);
console.log(`Wrote dedicated cloud webhook parity spec to ${specPath}`);

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

  const pubSubIdRef = {
    fromPrimaryProxyPath: '$.data.pubSubWebhookSubscriptionCreate.webhookSubscription.id',
  };
  const eventBridgeIdRef = {
    fromProxyResponse: 'eventbridge-create-payload',
    path: '$.data.eventBridgeWebhookSubscriptionCreate.webhookSubscription.id',
  };

  return {
    scenarioId: 'webhook-subscription-dedicated-cloud-destinations',
    operationNames: [
      'pubSubWebhookSubscriptionCreate',
      'pubSubWebhookSubscriptionUpdate',
      'eventBridgeWebhookSubscriptionCreate',
      'eventBridgeWebhookSubscriptionUpdate',
    ],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'user-errors-parity', 'read-after-write', 'downstream-read-parity'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['test/parity_test.gleam', 'test/shopify_draft_proxy/proxy/webhooks_test.gleam'],
    proxyRequest: {
      documentPath: pubSubCreateRequestPath,
      variablesCapturePath: '$.pubSubLifecycle.create.variables',
      apiVersion,
      headers,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Strict parity for deprecated dedicated Pub/Sub and EventBridge webhook subscription roots. The replay proves typed input normalization into the shared webhook subscription model, top-level cloud callbackUrl placeholder projection, downstream detail reads, and captured dedicated validation field paths without runtime Shopify writes.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'pubsub-create-payload',
          capturePath: '$.pubSubLifecycle.create.response.payload.data.pubSubWebhookSubscriptionCreate',
          proxyPath: '$.data.pubSubWebhookSubscriptionCreate',
          expectedDifferences: [idDifference, createdAtDifference, updatedAtDifference],
        },
        {
          name: 'pubsub-detail-after-create',
          capturePath: '$.pubSubLifecycle.detailAfterCreate.response.payload.data.webhookSubscription',
          proxyPath: '$.data.webhookSubscription',
          proxyRequest: {
            documentPath: detailRequestPath,
            variables: {
              id: pubSubIdRef,
            },
            apiVersion,
            headers,
          },
          expectedDifferences: detailDifferences,
        },
        validationTarget(
          'pubsub-create-bad-project',
          '$.validation.createPubSubBadProject.response.payload.data.pubSubWebhookSubscriptionCreate',
          pubSubCreateRequestPath,
          '$.validation.createPubSubBadProject.variables',
          headers,
        ),
        regressionValidationTarget(
          'regression-pubsub-create-bad-project-null-and-errors',
          '$.validation.createPubSubBadProject.response.payload.data.pubSubWebhookSubscriptionCreate',
          pubSubCreateRequestPath,
          '$.validation.createPubSubBadProject.variables',
          headers,
        ),
        validationTarget(
          'pubsub-create-bad-topic',
          '$.validation.createPubSubBadTopic.response.payload.data.pubSubWebhookSubscriptionCreate',
          pubSubCreateRequestPath,
          '$.validation.createPubSubBadTopic.variables',
          headers,
        ),
        pubSubUpdateTarget(
          'pubsub-update-bad-project',
          '$.validation.updatePubSubBadProject.response.payload.data.pubSubWebhookSubscriptionUpdate',
          '$.validation.updatePubSubBadProject.variables.webhookSubscription',
          pubSubIdRef,
          headers,
        ),
        regressionUpdateTarget(
          'regression-pubsub-update-bad-project-null-and-errors',
          pubSubUpdateTarget,
          '$.validation.updatePubSubBadProject.response.payload.data.pubSubWebhookSubscriptionUpdate',
          '$.validation.updatePubSubBadProject.variables.webhookSubscription',
          pubSubIdRef,
          headers,
        ),
        pubSubUpdateTarget(
          'pubsub-update-bad-topic',
          '$.validation.updatePubSubBadTopic.response.payload.data.pubSubWebhookSubscriptionUpdate',
          '$.validation.updatePubSubBadTopic.variables.webhookSubscription',
          pubSubIdRef,
          headers,
        ),
        {
          name: 'pubsub-update-payload',
          capturePath: '$.pubSubLifecycle.update.response.payload.data.pubSubWebhookSubscriptionUpdate',
          proxyPath: '$.data.pubSubWebhookSubscriptionUpdate',
          proxyRequest: {
            documentPath: pubSubUpdateRequestPath,
            variables: {
              id: pubSubIdRef,
              webhookSubscription: {
                fromCapturePath: '$.pubSubLifecycle.update.variables.webhookSubscription',
              },
            },
            apiVersion,
            headers,
          },
          expectedDifferences: [idDifference, createdAtDifference, updatedAtDifference],
        },
        {
          name: 'pubsub-detail-after-update',
          capturePath: '$.pubSubLifecycle.detailAfterUpdate.response.payload.data.webhookSubscription',
          proxyPath: '$.data.webhookSubscription',
          proxyRequest: {
            documentPath: detailRequestPath,
            variables: {
              id: pubSubIdRef,
            },
            apiVersion,
            headers,
          },
          expectedDifferences: detailDifferences,
        },
        {
          name: 'eventbridge-create-payload',
          capturePath: '$.eventBridgeLifecycle.create.response.payload.data.eventBridgeWebhookSubscriptionCreate',
          proxyPath: '$.data.eventBridgeWebhookSubscriptionCreate',
          proxyRequest: {
            documentPath: eventBridgeCreateRequestPath,
            variablesCapturePath: '$.eventBridgeLifecycle.create.variables',
            apiVersion,
            headers,
          },
          expectedDifferences: [idDifference, createdAtDifference, updatedAtDifference],
        },
        {
          name: 'eventbridge-detail-after-create',
          capturePath: '$.eventBridgeLifecycle.detailAfterCreate.response.payload.data.webhookSubscription',
          proxyPath: '$.data.webhookSubscription',
          proxyRequest: {
            documentPath: detailRequestPath,
            variables: {
              id: eventBridgeIdRef,
            },
            apiVersion,
            headers,
          },
          expectedDifferences: detailDifferences,
        },
        validationTarget(
          'eventbridge-create-malformed-arn',
          '$.validation.createEventBridgeMalformedArn.response.payload.data.eventBridgeWebhookSubscriptionCreate',
          eventBridgeCreateRequestPath,
          '$.validation.createEventBridgeMalformedArn.variables',
          headers,
        ),
        regressionValidationTarget(
          'regression-eventbridge-create-malformed-arn-null-and-errors',
          '$.validation.createEventBridgeMalformedArn.response.payload.data.eventBridgeWebhookSubscriptionCreate',
          eventBridgeCreateRequestPath,
          '$.validation.createEventBridgeMalformedArn.variables',
          headers,
        ),
        validationTarget(
          'eventbridge-create-wrong-api-client',
          '$.validation.createEventBridgeWrongApiClient.response.payload.data.eventBridgeWebhookSubscriptionCreate',
          eventBridgeCreateRequestPath,
          '$.validation.createEventBridgeWrongApiClient.variables',
          headers,
        ),
        regressionValidationTarget(
          'regression-eventbridge-create-wrong-api-client-null-and-errors',
          '$.validation.createEventBridgeWrongApiClient.response.payload.data.eventBridgeWebhookSubscriptionCreate',
          eventBridgeCreateRequestPath,
          '$.validation.createEventBridgeWrongApiClient.variables',
          headers,
        ),
        eventBridgeUpdateTarget(
          'eventbridge-update-malformed-arn',
          '$.validation.updateEventBridgeMalformedArn.response.payload.data.eventBridgeWebhookSubscriptionUpdate',
          '$.validation.updateEventBridgeMalformedArn.variables.webhookSubscription',
          eventBridgeIdRef,
          headers,
        ),
        regressionUpdateTarget(
          'regression-eventbridge-update-malformed-arn-null-and-errors',
          eventBridgeUpdateTarget,
          '$.validation.updateEventBridgeMalformedArn.response.payload.data.eventBridgeWebhookSubscriptionUpdate',
          '$.validation.updateEventBridgeMalformedArn.variables.webhookSubscription',
          eventBridgeIdRef,
          headers,
        ),
        eventBridgeUpdateTarget(
          'eventbridge-update-wrong-api-client',
          '$.validation.updateEventBridgeWrongApiClient.response.payload.data.eventBridgeWebhookSubscriptionUpdate',
          '$.validation.updateEventBridgeWrongApiClient.variables.webhookSubscription',
          eventBridgeIdRef,
          headers,
        ),
        regressionUpdateTarget(
          'regression-eventbridge-update-wrong-api-client-null-and-errors',
          eventBridgeUpdateTarget,
          '$.validation.updateEventBridgeWrongApiClient.response.payload.data.eventBridgeWebhookSubscriptionUpdate',
          '$.validation.updateEventBridgeWrongApiClient.variables.webhookSubscription',
          eventBridgeIdRef,
          headers,
        ),
        {
          name: 'eventbridge-update-payload',
          capturePath: '$.eventBridgeLifecycle.update.response.payload.data.eventBridgeWebhookSubscriptionUpdate',
          proxyPath: '$.data.eventBridgeWebhookSubscriptionUpdate',
          proxyRequest: {
            documentPath: eventBridgeUpdateRequestPath,
            variables: {
              id: eventBridgeIdRef,
              webhookSubscription: {
                fromCapturePath: '$.eventBridgeLifecycle.update.variables.webhookSubscription',
              },
            },
            apiVersion,
            headers,
          },
          expectedDifferences: [idDifference, createdAtDifference, updatedAtDifference],
        },
        {
          name: 'eventbridge-detail-after-update',
          capturePath: '$.eventBridgeLifecycle.detailAfterUpdate.response.payload.data.webhookSubscription',
          proxyPath: '$.data.webhookSubscription',
          proxyRequest: {
            documentPath: detailRequestPath,
            variables: {
              id: eventBridgeIdRef,
            },
            apiVersion,
            headers,
          },
          expectedDifferences: detailDifferences,
        },
      ],
    },
  };
}

function validationTarget(
  name: string,
  capturePath: string,
  documentPath: string,
  variablesCapturePath: string,
  headers: Record<string, string>,
): Record<string, unknown> {
  return {
    name,
    capturePath,
    proxyPath: rootPayloadProxyPath(capturePath),
    proxyRequest: {
      documentPath,
      variablesCapturePath,
      apiVersion,
      headers,
    },
  };
}

function regressionValidationTarget(
  name: string,
  capturePath: string,
  documentPath: string,
  variablesCapturePath: string,
  headers: Record<string, string>,
): Record<string, unknown> {
  return {
    ...validationTarget(name, capturePath, documentPath, variablesCapturePath, headers),
    selectedPaths: ['$.webhookSubscription', '$.userErrors'],
  };
}

function pubSubUpdateTarget(
  name: string,
  capturePath: string,
  inputCapturePath: string,
  idRef: Record<string, string>,
  headers: Record<string, string>,
): Record<string, unknown> {
  return {
    name,
    capturePath,
    proxyPath: '$.data.pubSubWebhookSubscriptionUpdate',
    proxyRequest: {
      documentPath: pubSubUpdateRequestPath,
      variables: {
        id: idRef,
        webhookSubscription: {
          fromCapturePath: inputCapturePath,
        },
      },
      apiVersion,
      headers,
    },
  };
}

function regressionUpdateTarget(
  name: string,
  targetBuilder: (
    name: string,
    capturePath: string,
    inputCapturePath: string,
    idRef: Record<string, string>,
    headers: Record<string, string>,
  ) => Record<string, unknown>,
  capturePath: string,
  inputCapturePath: string,
  idRef: Record<string, string>,
  headers: Record<string, string>,
): Record<string, unknown> {
  return {
    ...targetBuilder(name, capturePath, inputCapturePath, idRef, headers),
    selectedPaths: ['$.webhookSubscription', '$.userErrors'],
  };
}

function eventBridgeUpdateTarget(
  name: string,
  capturePath: string,
  inputCapturePath: string,
  idRef: Record<string, string>,
  headers: Record<string, string>,
): Record<string, unknown> {
  return {
    name,
    capturePath,
    proxyPath: '$.data.eventBridgeWebhookSubscriptionUpdate',
    proxyRequest: {
      documentPath: eventBridgeUpdateRequestPath,
      variables: {
        id: idRef,
        webhookSubscription: {
          fromCapturePath: inputCapturePath,
        },
      },
      apiVersion,
      headers,
    },
  };
}

function rootPayloadProxyPath(capturePath: string): string {
  if (capturePath.endsWith('.pubSubWebhookSubscriptionCreate')) {
    return '$.data.pubSubWebhookSubscriptionCreate';
  }
  return '$.data.eventBridgeWebhookSubscriptionCreate';
}
