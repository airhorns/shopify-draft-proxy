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
const outputPath = path.join(outputDir, 'webhook-subscription-scheme-allowlist.json');
const specPath = path.join('config', 'parity-specs', 'webhooks', 'webhook-subscription-scheme-allowlist.json');

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
      query FindDisposableWebhookSchemeAllowlistSubscriptions {
        webhookSubscriptions(first: 50, query: "uri:hermes-webhook-scheme-allowlist") {
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
      node['uri'].includes('hermes-webhook-scheme-allowlist')
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

  return subscription['id'];
}

function assertUserError(
  captureResult: CapturedRequest,
  rootField: 'webhookSubscriptionCreate' | 'webhookSubscriptionUpdate',
  expectedMessage: string,
): void {
  const data = captureResult.response.payload.data;
  if (!isObject(data)) {
    throw new Error(`${captureResult.documentPath} did not return data.`);
  }

  const payload = data[rootField];
  if (!isObject(payload)) {
    throw new Error(`${captureResult.documentPath} did not return ${rootField}.`);
  }

  const webhookSubscription = payload['webhookSubscription'];
  const userErrors = payload['userErrors'];
  if (webhookSubscription !== null || !Array.isArray(userErrors) || userErrors.length !== 1) {
    throw new Error(`${rootField} did not reject as expected: ${JSON.stringify(payload)}`);
  }

  const error = userErrors[0];
  if (!isObject(error) || !Array.isArray(error['field']) || typeof error['message'] !== 'string') {
    throw new Error(`${rootField} returned malformed userError: ${JSON.stringify(error)}`);
  }

  if (JSON.stringify(error['field']) !== JSON.stringify(['webhookSubscription', 'callbackUrl'])) {
    throw new Error(`${rootField} returned unexpected field ${JSON.stringify(error['field'])}`);
  }

  if (error['message'] !== expectedMessage) {
    throw new Error(`${rootField} returned unexpected message ${error['message']}`);
  }
}

function requireCase(cases: Record<string, CapturedRequest>, name: string): CapturedRequest {
  const captureResult = cases[name];
  if (captureResult === undefined) {
    throw new Error(`Missing captured case ${name}`);
  }

  return captureResult;
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
const setupCreateVariables = createVariables(`https://example.com/hermes-webhook-scheme-allowlist-${suffix}`);

let setupCreate: CapturedRequest | null = null;
let cleanup: CapturedRequest | null = null;
let setupId: string | null = null;
const preCaptureCleanup = await cleanupExistingDisposableWebhooks();

try {
  setupCreate = await capture(createRequestPath, setupCreateVariables);
  setupId = readCreatedWebhookId(setupCreate);

  const cases: Record<string, CapturedRequest> = {
    createFtpSchemeRejected: await capture(createRequestPath, createVariables('ftp://webhook-test.example.com/hook')),
    createWsSchemeRejected: await capture(createRequestPath, createVariables('ws://webhook-test.example.com/hook')),
    createBareHttpsInvalid: await capture(createRequestPath, createVariables('https://')),
    updateFtpSchemeRejected: await capture(
      updateRequestPath,
      updateVariables(setupId, 'ftp://webhook-test.example.com/hook'),
    ),
  };

  assertUserError(
    requireCase(cases, 'createFtpSchemeRejected'),
    'webhookSubscriptionCreate',
    'Address protocol ftp:// is not supported',
  );
  assertUserError(
    requireCase(cases, 'createWsSchemeRejected'),
    'webhookSubscriptionCreate',
    'Address protocol ws:// is not supported',
  );
  assertUserError(requireCase(cases, 'createBareHttpsInvalid'), 'webhookSubscriptionCreate', 'Address is invalid');
  assertUserError(
    requireCase(cases, 'updateFtpSchemeRejected'),
    'webhookSubscriptionUpdate',
    'Address protocol ftp:// is not supported',
  );

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
          'Captures webhookSubscriptionCreate/update URI scheme allowlist validation against live Shopify.',
          'The setup webhook uses an example.com HTTPS URI and is deleted before the fixture is written.',
          'No webhook delivery is intentionally triggered by this validation capture.',
        ],
        preCaptureCleanup,
        setup: {
          create: setupCreate,
          cleanup,
        },
        cases,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  await writeFile(specPath, `${JSON.stringify(buildSpec(), null, 2)}\n`, 'utf8');
} finally {
  if (setupId !== null && cleanup === null) {
    await capture(deleteRequestPath, { id: setupId });
  }
}

console.log(`Wrote webhook subscription scheme allowlist fixture to ${outputPath}`);
console.log(`Wrote webhook subscription scheme allowlist parity spec to ${specPath}`);

function buildSpec(): Record<string, unknown> {
  return {
    scenarioId: 'webhook-subscription-scheme-allowlist',
    operationNames: ['webhookSubscriptionCreate', 'webhookSubscriptionUpdate'],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity', 'payload-shape'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['tests/graphql_routes/admin_graphql_webhooks.rs'],
    proxyRequest: {
      documentPath: createRequestPath,
      apiVersion,
      variablesCapturePath: '$.setup.create.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Strict parity for webhook subscription URI scheme allowlist validation. The primary request creates a temporary local subscription so update validation can run against an existing proxy ID; comparison targets cover rejected create/update branches.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        createTarget('create-ftp-scheme-rejected', 'createFtpSchemeRejected'),
        createTarget('create-ws-scheme-rejected', 'createWsSchemeRejected'),
        createTarget('create-bare-https-invalid', 'createBareHttpsInvalid'),
        updateTarget('update-ftp-scheme-rejected', 'updateFtpSchemeRejected'),
      ],
    },
  };
}

function createTarget(name: string, caseName: string): Record<string, unknown> {
  return {
    name,
    capturePath: `$.cases.${caseName}.response.payload.data.webhookSubscriptionCreate`,
    proxyPath: '$.data.webhookSubscriptionCreate',
    proxyRequest: {
      documentPath: createRequestPath,
      variablesCapturePath: `$.cases.${caseName}.variables`,
    },
  };
}

function updateTarget(name: string, caseName: string): Record<string, unknown> {
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
    },
  };
}
