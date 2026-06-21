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

const scenarioId = 'eventbridge-cloud-format-json-only';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'webhooks');
const outputPath = path.join(outputDir, `${scenarioId}.json`);
const specPath = path.join('config', 'parity-specs', 'webhooks', `${scenarioId}.json`);

const requestDir = path.join('config', 'parity-requests', 'webhooks');
const dedicatedCreateRequestPath = path.join(requestDir, 'eventBridgeWebhookSubscriptionCreate-parity.graphql');
const dedicatedUpdateRequestPath = path.join(requestDir, 'eventBridgeWebhookSubscriptionUpdate-parity.graphql');
const unifiedCreateRequestPath = path.join(requestDir, 'webhookSubscriptionCreate-parity.graphql');
const unifiedUpdateRequestPath = path.join(requestDir, 'webhookSubscriptionUpdate-parity.graphql');
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

function requireNoUserErrors(captureResult: CapturedRequest, rootName: string): void {
  const payload = readPayload(captureResult, rootName);
  const userErrors = payload['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }
  throw new Error(`${rootName} returned unexpected userErrors: ${JSON.stringify(payload)}`);
}

function requireFormatOnlyUserError(captureResult: CapturedRequest, rootName: string): void {
  const payload = readPayload(captureResult, rootName);
  const subscription = payload['webhookSubscription'];
  const userErrors = payload['userErrors'];
  if (
    subscription === null &&
    Array.isArray(userErrors) &&
    userErrors.length === 1 &&
    isObject(userErrors[0]) &&
    JSON.stringify(userErrors[0]['field']) === JSON.stringify(['webhookSubscription', 'format']) &&
    userErrors[0]['message'] === "Format can only be used with format: 'json'"
  ) {
    return;
  }

  throw new Error(`${rootName} did not return the expected format userError: ${JSON.stringify(payload)}`);
}

function extractRequestingApiClientId(captureResult: CapturedRequest): string {
  const serialized = JSON.stringify(captureResult.response.payload);
  const match = serialized.match(/instead of '(\d+)'/u);
  if (!match?.[1]) {
    throw new Error(`Could not derive requesting api_client_id from ${serialized}`);
  }

  return match[1];
}

function eventBridgeArn(region: string, apiClientId: string, sourceName: string): string {
  return `arn:aws:events:${region}::event-source/aws.partner/shopify.com/${apiClientId}/${sourceName}`;
}

function dedicatedCreateVariables(arn: string, format = 'JSON'): Record<string, unknown> {
  return {
    topic: 'SHOP_UPDATE',
    webhookSubscription: {
      arn,
      format,
    },
  };
}

function dedicatedUpdateVariables(id: string, arn: string, format = 'JSON'): Record<string, unknown> {
  return {
    id,
    webhookSubscription: {
      arn,
      format,
    },
  };
}

function unifiedCreateVariables(arn: string, format = 'JSON'): Record<string, unknown> {
  return {
    topic: 'SHOP_UPDATE',
    webhookSubscription: {
      uri: arn,
      format,
    },
  };
}

function unifiedUpdateVariables(id: string, arn: string, format = 'JSON'): Record<string, unknown> {
  return {
    id,
    webhookSubscription: {
      uri: arn,
      format,
    },
  };
}

const runId = Date.now().toString(36);
const wrongApiClientProbeArn = eventBridgeArn('us-east-1', '1', `format-probe-${runId}`);

const setup: {
  apiClientProbe: CapturedRequest | null;
  dedicatedUpdateSetup: CapturedRequest | null;
  unifiedUpdateSetup: CapturedRequest | null;
  cleanupDedicatedUpdateSetup: CapturedRequest | null;
  cleanupUnifiedUpdateSetup: CapturedRequest | null;
} = {
  apiClientProbe: null,
  dedicatedUpdateSetup: null,
  unifiedUpdateSetup: null,
  cleanupDedicatedUpdateSetup: null,
  cleanupUnifiedUpdateSetup: null,
};
const validation: {
  dedicatedCreateXmlRejected: CapturedRequest | null;
  dedicatedUpdateXmlRejected: CapturedRequest | null;
  unifiedCreateXmlRejected: CapturedRequest | null;
  unifiedUpdateXmlRejected: CapturedRequest | null;
} = {
  dedicatedCreateXmlRejected: null,
  dedicatedUpdateXmlRejected: null,
  unifiedCreateXmlRejected: null,
  unifiedUpdateXmlRejected: null,
};

let requestingApiClientId: string | null = null;
let dedicatedUpdateSetupId: string | null = null;
let unifiedUpdateSetupId: string | null = null;

try {
  setup.apiClientProbe = await capture(dedicatedCreateRequestPath, dedicatedCreateVariables(wrongApiClientProbeArn));
  requestingApiClientId = extractRequestingApiClientId(setup.apiClientProbe);

  validation.dedicatedCreateXmlRejected = await capture(
    dedicatedCreateRequestPath,
    dedicatedCreateVariables(
      eventBridgeArn('us-east-1', requestingApiClientId, `xml-dedicated-create-${runId}`),
      'XML',
    ),
  );
  requireFormatOnlyUserError(validation.dedicatedCreateXmlRejected, 'eventBridgeWebhookSubscriptionCreate');

  validation.unifiedCreateXmlRejected = await capture(
    unifiedCreateRequestPath,
    unifiedCreateVariables(eventBridgeArn('us-east-1', requestingApiClientId, `xml-unified-create-${runId}`), 'XML'),
  );
  requireFormatOnlyUserError(validation.unifiedCreateXmlRejected, 'webhookSubscriptionCreate');

  setup.dedicatedUpdateSetup = await capture(
    dedicatedCreateRequestPath,
    dedicatedCreateVariables(
      eventBridgeArn('us-east-1', requestingApiClientId, `json-dedicated-update-setup-${runId}`),
    ),
  );
  requireNoUserErrors(setup.dedicatedUpdateSetup, 'eventBridgeWebhookSubscriptionCreate');
  dedicatedUpdateSetupId = readCreatedWebhookId(setup.dedicatedUpdateSetup, 'eventBridgeWebhookSubscriptionCreate');

  validation.dedicatedUpdateXmlRejected = await capture(
    dedicatedUpdateRequestPath,
    dedicatedUpdateVariables(
      dedicatedUpdateSetupId,
      eventBridgeArn('us-west-2', requestingApiClientId, `xml-dedicated-update-${runId}`),
      'XML',
    ),
  );
  requireFormatOnlyUserError(validation.dedicatedUpdateXmlRejected, 'eventBridgeWebhookSubscriptionUpdate');

  setup.unifiedUpdateSetup = await capture(
    unifiedCreateRequestPath,
    unifiedCreateVariables(eventBridgeArn('us-east-1', requestingApiClientId, `json-unified-update-setup-${runId}`)),
  );
  requireNoUserErrors(setup.unifiedUpdateSetup, 'webhookSubscriptionCreate');
  unifiedUpdateSetupId = readCreatedWebhookId(setup.unifiedUpdateSetup, 'webhookSubscriptionCreate');

  validation.unifiedUpdateXmlRejected = await capture(
    unifiedUpdateRequestPath,
    unifiedUpdateVariables(
      unifiedUpdateSetupId,
      eventBridgeArn('us-west-2', requestingApiClientId, `xml-unified-update-${runId}`),
      'XML',
    ),
  );
  requireFormatOnlyUserError(validation.unifiedUpdateXmlRejected, 'webhookSubscriptionUpdate');

  setup.cleanupDedicatedUpdateSetup = await capture(deleteRequestPath, { id: dedicatedUpdateSetupId });
  setup.cleanupUnifiedUpdateSetup = await capture(deleteRequestPath, { id: unifiedUpdateSetupId });

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
          'Captures Shopify Admin GraphQL EventBridge ARN cloud-delivery format validation for dedicated and unified webhook subscription create/update roots.',
          'All ARN + XML branches return webhookSubscription: null and one format userError on ["webhookSubscription", "format"].',
          'The script creates two temporary JSON EventBridge webhook subscriptions only to provide update targets, deletes both during cleanup, and does not trigger webhook delivery.',
        ],
        setup,
        validation,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  await writeFile(specPath, `${JSON.stringify(buildSpec(requestingApiClientId), null, 2)}\n`, 'utf8');
} finally {
  if (dedicatedUpdateSetupId !== null && setup.cleanupDedicatedUpdateSetup === null) {
    await capture(deleteRequestPath, { id: dedicatedUpdateSetupId });
  }
  if (unifiedUpdateSetupId !== null && setup.cleanupUnifiedUpdateSetup === null) {
    await capture(deleteRequestPath, { id: unifiedUpdateSetupId });
  }
}

console.log(`Wrote EventBridge cloud-format fixture to ${outputPath}`);
console.log(`Wrote EventBridge cloud-format parity spec to ${specPath}`);

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
  const dedicatedIdRef = {
    fromPrimaryProxyPath: '$.data.eventBridgeWebhookSubscriptionCreate.webhookSubscription.id',
  };
  const unifiedIdRef = {
    fromProxyResponse: 'unified-update-setup-json-succeeds',
    path: '$.data.webhookSubscriptionCreate.webhookSubscription.id',
  };

  return {
    scenarioId,
    operationNames: [
      'eventBridgeWebhookSubscriptionCreate',
      'eventBridgeWebhookSubscriptionUpdate',
      'webhookSubscriptionCreate',
      'webhookSubscriptionUpdate',
    ],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity', 'payload-shape'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['tests/graphql_routes/admin_graphql_webhooks.rs'],
    proxyRequest: {
      documentPath: dedicatedCreateRequestPath,
      variablesCapturePath: '$.setup.dedicatedUpdateSetup.variables',
      apiVersion,
      headers,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Strict parity for the EventBridge ARN cloud-delivery JSON-only format rule across dedicated EventBridge and unified webhook subscription create/update roots. The JSON setup creates are replayed locally only to obtain IDs for update rejection checks.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        setupTarget(
          'dedicated-update-setup-json-succeeds',
          '$.setup.dedicatedUpdateSetup.response.payload.data.eventBridgeWebhookSubscriptionCreate',
          '$.data.eventBridgeWebhookSubscriptionCreate',
          dedicatedCreateRequestPath,
          '$.setup.dedicatedUpdateSetup.variables',
          headers,
          [idDifference, createdAtDifference, updatedAtDifference],
        ),
        createValidationTarget(
          'dedicated-create-arn-xml-rejected',
          '$.validation.dedicatedCreateXmlRejected.response.payload.data.eventBridgeWebhookSubscriptionCreate',
          '$.data.eventBridgeWebhookSubscriptionCreate',
          dedicatedCreateRequestPath,
          '$.validation.dedicatedCreateXmlRejected.variables',
          headers,
        ),
        updateValidationTarget(
          'dedicated-update-arn-xml-rejected',
          '$.validation.dedicatedUpdateXmlRejected.response.payload.data.eventBridgeWebhookSubscriptionUpdate',
          '$.data.eventBridgeWebhookSubscriptionUpdate',
          dedicatedUpdateRequestPath,
          dedicatedIdRef,
          '$.validation.dedicatedUpdateXmlRejected.variables.webhookSubscription',
          headers,
        ),
        setupTarget(
          'unified-update-setup-json-succeeds',
          '$.setup.unifiedUpdateSetup.response.payload.data.webhookSubscriptionCreate',
          '$.data.webhookSubscriptionCreate',
          unifiedCreateRequestPath,
          '$.setup.unifiedUpdateSetup.variables',
          headers,
          [idDifference, createdAtDifference, updatedAtDifference],
        ),
        createValidationTarget(
          'unified-create-arn-xml-rejected',
          '$.validation.unifiedCreateXmlRejected.response.payload.data.webhookSubscriptionCreate',
          '$.data.webhookSubscriptionCreate',
          unifiedCreateRequestPath,
          '$.validation.unifiedCreateXmlRejected.variables',
          headers,
        ),
        updateValidationTarget(
          'unified-update-arn-xml-rejected',
          '$.validation.unifiedUpdateXmlRejected.response.payload.data.webhookSubscriptionUpdate',
          '$.data.webhookSubscriptionUpdate',
          unifiedUpdateRequestPath,
          unifiedIdRef,
          '$.validation.unifiedUpdateXmlRejected.variables.webhookSubscription',
          headers,
        ),
      ],
    },
  };
}

function setupTarget(
  name: string,
  capturePath: string,
  proxyPath: string,
  documentPath: string,
  variablesCapturePath: string,
  headers: Record<string, string>,
  expectedDifferences: Record<string, unknown>[],
): Record<string, unknown> {
  return {
    name,
    capturePath,
    proxyPath,
    proxyRequest: {
      documentPath,
      variablesCapturePath,
      apiVersion,
      headers,
    },
    expectedDifferences,
  };
}

function createValidationTarget(
  name: string,
  capturePath: string,
  proxyPath: string,
  documentPath: string,
  variablesCapturePath: string,
  headers: Record<string, string>,
): Record<string, unknown> {
  return {
    name,
    capturePath,
    proxyPath,
    proxyRequest: {
      documentPath,
      variablesCapturePath,
      apiVersion,
      headers,
    },
  };
}

function updateValidationTarget(
  name: string,
  capturePath: string,
  proxyPath: string,
  documentPath: string,
  idRef: Record<string, string>,
  inputCapturePath: string,
  headers: Record<string, string>,
): Record<string, unknown> {
  return {
    name,
    capturePath,
    proxyPath,
    proxyRequest: {
      documentPath,
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
