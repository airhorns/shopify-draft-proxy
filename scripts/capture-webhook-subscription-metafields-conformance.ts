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

type MetafieldIdentifier = {
  namespace: string;
  key: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'webhooks');
const outputPath = path.join(outputDir, 'webhook-subscription-metafields-lifecycle.json');
const specPath = path.join('config', 'parity-specs', 'webhooks', 'webhook-subscription-metafields-lifecycle.json');

const requestDir = path.join('config', 'parity-requests', 'webhooks');
const createRequestPath = path.join(requestDir, 'webhook-subscription-metafields-create.graphql');
const updateRequestPath = path.join(requestDir, 'webhook-subscription-metafields-update.graphql');
const detailRequestPath = path.join(requestDir, 'webhook-subscription-metafields-detail-read.graphql');
const listRequestPath = path.join(requestDir, 'webhook-subscription-metafields-list.graphql');
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

async function captureDocument(
  documentPath: string,
  document: string,
  variables: Record<string, unknown>,
): Promise<CapturedRequest> {
  const response = await runGraphqlRequest(document, variables);

  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`${documentPath} failed: ${JSON.stringify(response.payload)}`);
  }

  return { documentPath, variables, response };
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readWebhookId(captureResult: CapturedRequest, rootName: string): string {
  const data = captureResult.response.payload.data;
  if (!isObject(data)) {
    throw new Error(`${rootName} response did not include data.`);
  }

  const payload = data[rootName];
  if (!isObject(payload)) {
    throw new Error(`${rootName} payload is missing.`);
  }

  const subscription = payload['webhookSubscription'];
  if (!isObject(subscription) || typeof subscription['id'] !== 'string') {
    throw new Error(`${rootName} did not return webhookSubscription.id.`);
  }

  return subscription['id'];
}

function readPayloadMetafields(captureResult: CapturedRequest, rootName: string): MetafieldIdentifier[] {
  const data = captureResult.response.payload.data;
  if (!isObject(data)) {
    throw new Error(`${rootName} response did not include data.`);
  }

  const payload = data[rootName];
  if (!isObject(payload)) {
    throw new Error(`${rootName} payload is missing.`);
  }

  const subscription = payload['webhookSubscription'];
  if (!isObject(subscription) || !Array.isArray(subscription['metafields'])) {
    throw new Error(`${rootName} did not return webhookSubscription.metafields.`);
  }

  return subscription['metafields'].filter((entry): entry is MetafieldIdentifier => {
    return isObject(entry) && typeof entry['namespace'] === 'string' && typeof entry['key'] === 'string';
  });
}

function createVariables(uri: string, metafields?: MetafieldIdentifier[]): Record<string, unknown> {
  const webhookSubscription: Record<string, unknown> = {
    uri,
    format: 'JSON',
  };
  if (metafields !== undefined) {
    webhookSubscription['metafields'] = metafields;
  }

  return {
    topic: 'SHOP_UPDATE',
    webhookSubscription,
  };
}

function updateVariables(id: string, uri: string, metafields: MetafieldIdentifier[]): Record<string, unknown> {
  return {
    id,
    webhookSubscription: {
      uri,
      format: 'JSON',
      metafields,
    },
  };
}

const schemaProbeDocument = `#graphql
  query WebhookSubscriptionMetafieldsSchemaProbe {
    webhookSubscriptionInput: __type(name: "WebhookSubscriptionInput") {
      inputFields {
        name
      }
    }
    webhookSubscription: __type(name: "WebhookSubscription") {
      fields {
        name
      }
    }
    metafieldIdentifier: __type(name: "MetafieldIdentifier") {
      fields {
        name
      }
    }
  }
`;

function assertSchemaExposesMetafields(schemaProbe: CapturedRequest): void {
  const data = schemaProbe.response.payload.data;
  const inputFields =
    isObject(data) &&
    isObject(data['webhookSubscriptionInput']) &&
    Array.isArray(data['webhookSubscriptionInput']['inputFields'])
      ? data['webhookSubscriptionInput']['inputFields']
      : [];
  const outputFields =
    isObject(data) && isObject(data['webhookSubscription']) && Array.isArray(data['webhookSubscription']['fields'])
      ? data['webhookSubscription']['fields']
      : [];

  const hasInput = inputFields.some((field) => isObject(field) && field['name'] === 'metafields');
  const hasOutput = outputFields.some((field) => isObject(field) && field['name'] === 'metafields');
  if (!hasInput || !hasOutput) {
    throw new Error(`WebhookSubscription metafields is not public in ${apiVersion}.`);
  }
}

const runId = Date.now().toString(36);
const createUri = `https://example.com/hermes-webhook-metafields-${runId}`;
const updateUri = `${createUri}-updated`;
const omittedUri = `${createUri}-omitted`;
const createMetafields = [{ namespace: 'custom', key: `color${runId}` }];
const updateMetafields = [{ namespace: 'custom', key: `material${runId}` }];

let createdId: string | null = null;
let omittedId: string | null = null;
let cleanup: CapturedRequest | null = null;
let omittedCleanup: CapturedRequest | null = null;

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

const omittedInput: {
  create: CapturedRequest | null;
  listAfterCreate: CapturedRequest | null;
  delete: CapturedRequest | null;
} = {
  create: null,
  listAfterCreate: null,
  delete: null,
};

try {
  const schemaProbe = await captureDocument(
    'inline:webhook-subscription-metafields-schema-probe',
    schemaProbeDocument,
    {},
  );
  assertSchemaExposesMetafields(schemaProbe);

  lifecycle.create = await capture(createRequestPath, createVariables(createUri, createMetafields));
  createdId = readWebhookId(lifecycle.create, 'webhookSubscriptionCreate');
  lifecycle.detailAfterCreate = await capture(detailRequestPath, { id: createdId });
  lifecycle.listAfterCreate = await capture(listRequestPath, { first: 5, uri: createUri });
  lifecycle.update = await capture(updateRequestPath, updateVariables(createdId, updateUri, updateMetafields));
  lifecycle.detailAfterUpdate = await capture(detailRequestPath, { id: createdId });
  lifecycle.listAfterUpdate = await capture(listRequestPath, { first: 5, uri: updateUri });
  lifecycle.delete = await capture(deleteRequestPath, { id: createdId });
  cleanup = lifecycle.delete;

  omittedInput.create = await capture(createRequestPath, createVariables(omittedUri));
  omittedId = readWebhookId(omittedInput.create, 'webhookSubscriptionCreate');
  omittedInput.listAfterCreate = await capture(listRequestPath, { first: 5, uri: omittedUri });
  omittedInput.delete = await capture(deleteRequestPath, { id: omittedId });
  omittedCleanup = omittedInput.delete;

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        notes: [
          'Captures public WebhookSubscription.metafields input/output round-trip for API-created HTTP webhook subscriptions.',
          'The create branch stores and returns a MetafieldIdentifier list, then downstream detail and URI-filtered list reads project the same list.',
          'The update branch replaces the MetafieldIdentifier list and downstream reads project the replacement list.',
          'The omitted-input branch proves Shopify returns an empty non-null list when metafields is not supplied.',
          'Temporary subscriptions are deleted during cleanup and the script does not trigger webhook delivery.',
        ],
        schemaProbe,
        expectedMetafields: {
          create: readPayloadMetafields(lifecycle.create, 'webhookSubscriptionCreate'),
          update: readPayloadMetafields(lifecycle.update, 'webhookSubscriptionUpdate'),
          omittedCreate: readPayloadMetafields(omittedInput.create, 'webhookSubscriptionCreate'),
        },
        lifecycle,
        omittedInput,
        cleanup,
        omittedCleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  await writeFile(specPath, `${JSON.stringify(buildSpec(), null, 2)}\n`, 'utf8');
} finally {
  if (createdId !== null && cleanup === null) {
    await capture(deleteRequestPath, { id: createdId });
  }
  if (omittedId !== null && omittedCleanup === null) {
    await capture(deleteRequestPath, { id: omittedId });
  }
}

console.log(`Wrote webhook metafields fixture to ${outputPath}`);
console.log(`Wrote webhook metafields parity spec to ${specPath}`);

function buildSpec(): Record<string, unknown> {
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
    scenarioId: 'webhook-subscription-metafields-lifecycle',
    operationNames: [
      'webhookSubscriptionCreate',
      'webhookSubscriptionUpdate',
      'webhookSubscription',
      'webhookSubscriptions',
    ],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'nullability-parity', 'read-after-write', 'downstream-read-parity'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['tests/graphql_routes/admin_graphql_webhooks.rs'],
    proxyRequest: {
      documentPath: createRequestPath,
      variablesCapturePath: '$.lifecycle.create.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Strict parity for WebhookSubscription.metafields input/output lifecycle. The scenario records real Shopify 2026-04 create, downstream detail/list reads, update replacement, and omitted-input empty-list behavior.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'create-payload-metafields',
          capturePath: '$.lifecycle.create.response.payload.data.webhookSubscriptionCreate',
          proxyPath: '$.data.webhookSubscriptionCreate',
          expectedDifferences: [idDifference, createdAtDifference, updatedAtDifference],
        },
        {
          name: 'detail-after-create-metafields',
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
          },
          expectedDifferences: [detailIdDifference, detailCreatedAtDifference, detailUpdatedAtDifference],
        },
        {
          name: 'list-after-create-metafields',
          capturePath: '$.lifecycle.listAfterCreate.response.payload.data.webhookSubscriptions',
          proxyPath: '$.data.webhookSubscriptions',
          proxyRequest: {
            documentPath: listRequestPath,
            variablesCapturePath: '$.lifecycle.listAfterCreate.variables',
            apiVersion,
          },
          expectedDifferences: [nodeIdDifference],
        },
        {
          name: 'update-payload-metafields',
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
          },
          expectedDifferences: [idDifference, createdAtDifference, updatedAtDifference],
        },
        {
          name: 'detail-after-update-metafields',
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
          },
          expectedDifferences: [detailIdDifference, detailCreatedAtDifference, detailUpdatedAtDifference],
        },
        {
          name: 'list-after-update-metafields',
          capturePath: '$.lifecycle.listAfterUpdate.response.payload.data.webhookSubscriptions',
          proxyPath: '$.data.webhookSubscriptions',
          proxyRequest: {
            documentPath: listRequestPath,
            variablesCapturePath: '$.lifecycle.listAfterUpdate.variables',
            apiVersion,
          },
          expectedDifferences: [nodeIdDifference],
        },
        {
          name: 'omitted-create-payload-empty-metafields',
          capturePath: '$.omittedInput.create.response.payload.data.webhookSubscriptionCreate',
          proxyPath: '$.data.webhookSubscriptionCreate',
          proxyRequest: {
            documentPath: createRequestPath,
            variablesCapturePath: '$.omittedInput.create.variables',
            apiVersion,
          },
          expectedDifferences: [idDifference, createdAtDifference, updatedAtDifference],
        },
        {
          name: 'omitted-list-read-empty-metafields',
          capturePath: '$.omittedInput.listAfterCreate.response.payload.data.webhookSubscriptions',
          proxyPath: '$.data.webhookSubscriptions',
          proxyRequest: {
            documentPath: listRequestPath,
            variablesCapturePath: '$.omittedInput.listAfterCreate.variables',
            apiVersion,
          },
          expectedDifferences: [nodeIdDifference],
        },
      ],
    },
  };
}
