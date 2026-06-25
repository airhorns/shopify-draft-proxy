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

const maxFilterBytes = 65_535;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'webhooks');
const outputPath = path.join(outputDir, 'webhook-subscription-filter-byte-size-validation.json');
const specPath = path.join(
  'config',
  'parity-specs',
  'webhooks',
  'webhook-subscription-filter-byte-size-validation.json',
);

const createRequestPath = path.join(
  'config',
  'parity-requests',
  'webhooks',
  'webhookSubscriptionCreate-parity.graphql',
);
const updateRequestPath = path.join(
  'config',
  'parity-requests',
  'webhooks',
  'webhookSubscriptionUpdate-parity.graphql',
);
const detailRequestPath = path.join(
  'config',
  'parity-requests',
  'webhooks',
  'webhook-subscription-detail-read.graphql',
);
const deleteRequestPath = path.join(
  'config',
  'parity-requests',
  'webhooks',
  'webhookSubscriptionDelete-parity.graphql',
);

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readText(relativePath: string): Promise<string> {
  return readFile(path.join(process.cwd(), relativePath), 'utf8');
}

function wellFormedFilterWithByteSize(byteSize: number): string {
  if (byteSize < 'id:1'.length) {
    throw new Error(`Filter byte size ${byteSize} is too small for a field:value token.`);
  }

  const tokenCount = Math.max(1, Math.floor((byteSize + 1) / 'id:1 '.length));
  const filter = Array.from({ length: tokenCount }, () => 'id:1').join(' ');
  const paddingBytes = byteSize - Buffer.byteLength(filter, 'utf8');
  if (paddingBytes < 0) {
    throw new Error(`Generated filter exceeded requested byte size ${byteSize}.`);
  }

  return `${filter}${'1'.repeat(paddingBytes)}`;
}

function createVariables(uri: string, filter: string): Record<string, unknown> {
  return {
    topic: 'ORDERS_CREATE',
    webhookSubscription: {
      uri,
      format: 'JSON',
      filter,
    },
  };
}

function updateVariables(id: string, uri: string, filter: string): Record<string, unknown> {
  return {
    id,
    webhookSubscription: {
      uri,
      format: 'JSON',
      includeFields: ['id'],
      metafieldNamespaces: [],
      filter,
    },
  };
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

function readPayload(captureResult: CapturedRequest, root: string): Record<string, unknown> {
  const data = captureResult.response.payload.data;
  if (!isObject(data)) throw new Error(`${root} capture did not return data.`);

  const payload = data[root];
  if (!isObject(payload)) throw new Error(`${root} capture is missing payload.`);

  return payload;
}

function readCreatedWebhookId(captureResult: CapturedRequest, root: string): string | null {
  const payload = readPayload(captureResult, root);
  const webhookSubscription = payload['webhookSubscription'];
  if (!isObject(webhookSubscription)) return null;

  const id = webhookSubscription['id'];
  return typeof id === 'string' ? id : null;
}

function assertCreated(captureResult: CapturedRequest, root: string): string {
  const id = readCreatedWebhookId(captureResult, root);
  if (id === null) {
    throw new Error(
      `Expected ${root} capture to create a subscription: ${JSON.stringify(captureResult.response.payload)}`,
    );
  }

  const payload = readPayload(captureResult, root);
  if (!Array.isArray(payload['userErrors']) || payload['userErrors'].length !== 0) {
    throw new Error(`${root} returned userErrors: ${JSON.stringify(payload)}`);
  }

  return id;
}

function assertFilterTooLarge(captureResult: CapturedRequest, root: string): void {
  const payload = readPayload(captureResult, root);
  if (payload['webhookSubscription'] !== null) {
    throw new Error(`Oversized filter unexpectedly created/updated a subscription: ${JSON.stringify(payload)}`);
  }

  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 1 || !isObject(userErrors[0])) {
    throw new Error(`Oversized filter returned malformed userErrors: ${JSON.stringify(payload)}`);
  }

  const error = userErrors[0];
  if (
    JSON.stringify(error['field']) !== JSON.stringify(['webhookSubscription']) ||
    error['message'] !== 'The specified filter exceeds the maximum allowed size.'
  ) {
    throw new Error(`Oversized filter returned unexpected userError: ${JSON.stringify(error)}`);
  }
}

function assertDetailFilter(captureResult: CapturedRequest, expectedFilter: string): void {
  const data = captureResult.response.payload.data;
  if (!isObject(data)) throw new Error('Detail capture did not return data.');

  const subscription = data['webhookSubscription'];
  if (!isObject(subscription) || subscription['filter'] !== expectedFilter) {
    throw new Error(`Detail capture did not preserve expected filter: ${JSON.stringify(subscription)}`);
  }
}

const suffix = `${Date.now()}`;
const acceptedFilter = wellFormedFilterWithByteSize(maxFilterBytes);
const oversizedFilter = wellFormedFilterWithByteSize(maxFilterBytes + 1);
const updateBaseFilter = 'id:1';
const acceptedUri = `https://example.com/hermes-webhook-filter-size-${suffix}-accepted`;
const rejectedCreateUri = `https://example.com/hermes-webhook-filter-size-${suffix}-rejected-create`;
const updateUri = `https://example.com/hermes-webhook-filter-size-${suffix}-update`;

if (Buffer.byteLength(acceptedFilter, 'utf8') !== maxFilterBytes) {
  throw new Error('Accepted filter generator did not produce the expected byte size.');
}
if (Buffer.byteLength(oversizedFilter, 'utf8') !== maxFilterBytes + 1) {
  throw new Error('Oversized filter generator did not produce the expected byte size.');
}

let acceptedAtLimitId: string | null = null;
let createUpdateBaseId: string | null = null;
let cleanupAcceptedAtLimit: CapturedRequest | null = null;
let cleanupUpdateBase: CapturedRequest | null = null;

try {
  const acceptedAtLimit = await capture(createRequestPath, createVariables(acceptedUri, acceptedFilter));
  acceptedAtLimitId = assertCreated(acceptedAtLimit, 'webhookSubscriptionCreate');

  const rejectedCreateAboveLimit = await capture(
    createRequestPath,
    createVariables(rejectedCreateUri, oversizedFilter),
  );
  assertFilterTooLarge(rejectedCreateAboveLimit, 'webhookSubscriptionCreate');

  const createUpdateBase = await capture(createRequestPath, createVariables(updateUri, updateBaseFilter));
  createUpdateBaseId = assertCreated(createUpdateBase, 'webhookSubscriptionCreate');

  const rejectedUpdateAboveLimit = await capture(
    updateRequestPath,
    updateVariables(createUpdateBaseId, updateUri, oversizedFilter),
  );
  assertFilterTooLarge(rejectedUpdateAboveLimit, 'webhookSubscriptionUpdate');

  const detailAfterRejectedUpdate = await capture(detailRequestPath, { id: createUpdateBaseId });
  assertDetailFilter(detailAfterRejectedUpdate, updateBaseFilter);

  cleanupAcceptedAtLimit = await capture(deleteRequestPath, { id: acceptedAtLimitId });
  cleanupUpdateBase = await capture(deleteRequestPath, { id: createUpdateBaseId });

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        notes: [
          'Captures Shopify webhook subscription filter byte-size validation at the MySQL text-column limit.',
          `The accepted create uses a syntactically valid filter with exactly ${maxFilterBytes} UTF-8 bytes.`,
          `The rejected create and update use a syntactically valid filter with ${maxFilterBytes + 1} UTF-8 bytes.`,
          'The post-update detail read proves the rejected update does not store the oversized filter.',
          'No webhook delivery is intentionally triggered by this validation capture.',
        ],
        cases: {
          acceptedAtLimit,
          rejectedCreateAboveLimit,
          createUpdateBase,
          rejectedUpdateAboveLimit,
          detailAfterRejectedUpdate,
        },
        cleanup: {
          acceptedAtLimit: cleanupAcceptedAtLimit,
          updateBase: cleanupUpdateBase,
        },
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
  await writeFile(specPath, `${JSON.stringify(buildSpec(), null, 2)}\n`, 'utf8');
} finally {
  if (acceptedAtLimitId !== null && cleanupAcceptedAtLimit === null) {
    await capture(deleteRequestPath, { id: acceptedAtLimitId });
  }
  if (createUpdateBaseId !== null && cleanupUpdateBase === null) {
    await capture(deleteRequestPath, { id: createUpdateBaseId });
  }
}

console.log(`Wrote webhook subscription filter byte-size fixture to ${outputPath}`);
console.log(`Wrote webhook subscription filter byte-size parity spec to ${specPath}`);

function webhookSubscriptionExpectedDifferences(timestampReason: string): Record<string, unknown>[] {
  return [
    {
      path: '$.webhookSubscription.id',
      matcher: 'shopify-gid:WebhookSubscription',
      reason: "The proxy creates a stable synthetic webhook subscription ID instead of Shopify's live ID.",
    },
    {
      path: '$.webhookSubscription.createdAt',
      matcher: 'iso-timestamp',
      reason: 'The proxy uses its deterministic synthetic clock for locally staged webhook subscriptions.',
    },
    {
      path: '$.webhookSubscription.updatedAt',
      matcher: 'iso-timestamp',
      reason: timestampReason,
    },
  ];
}

function detailExpectedDifferences(): Record<string, unknown>[] {
  return [
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
      reason: 'The rejected update leaves the proxy synthetic update timestamp unchanged from the create.',
    },
  ];
}

function buildSpec(): Record<string, unknown> {
  return {
    scenarioId: 'webhook-subscription-filter-byte-size-validation',
    operationNames: ['webhookSubscriptionCreate', 'webhookSubscriptionUpdate', 'webhookSubscription'],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity', 'payload-shape', 'local-staging'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['tests/graphql_routes/admin_graphql_webhooks.rs'],
    proxyRequest: {
      documentPath: createRequestPath,
      apiVersion,
      variablesCapturePath: '$.cases.acceptedAtLimit.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Strict parity for webhook subscription filter byte-size validation. Shopify accepts a syntactically valid filter at 65,535 UTF-8 bytes, rejects create/update filters at 65,536 bytes with the distinct filter size userError, and leaves the subscription unchanged after a rejected update.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'accepted-at-limit-filter',
          capturePath: '$.cases.acceptedAtLimit.response.payload.data.webhookSubscriptionCreate',
          proxyPath: '$.data.webhookSubscriptionCreate',
          expectedDifferences: webhookSubscriptionExpectedDifferences(
            'The proxy uses its deterministic synthetic clock for locally staged webhook subscriptions.',
          ),
        },
        {
          name: 'create-oversized-filter-user-error',
          capturePath: '$.cases.rejectedCreateAboveLimit.response.payload.data.webhookSubscriptionCreate',
          proxyPath: '$.data.webhookSubscriptionCreate',
          proxyRequest: {
            documentPath: createRequestPath,
            variablesCapturePath: '$.cases.rejectedCreateAboveLimit.variables',
          },
        },
        {
          name: 'create-update-base',
          capturePath: '$.cases.createUpdateBase.response.payload.data.webhookSubscriptionCreate',
          proxyPath: '$.data.webhookSubscriptionCreate',
          proxyRequest: {
            documentPath: createRequestPath,
            variablesCapturePath: '$.cases.createUpdateBase.variables',
          },
          expectedDifferences: webhookSubscriptionExpectedDifferences(
            'The proxy uses its deterministic synthetic clock for locally staged webhook subscriptions.',
          ),
        },
        {
          name: 'update-oversized-filter-user-error',
          capturePath: '$.cases.rejectedUpdateAboveLimit.response.payload.data.webhookSubscriptionUpdate',
          proxyPath: '$.data.webhookSubscriptionUpdate',
          proxyRequest: {
            documentPath: updateRequestPath,
            variables: {
              id: {
                fromProxyResponse: 'create-update-base',
                path: '$.data.webhookSubscriptionCreate.webhookSubscription.id',
              },
              webhookSubscription: {
                fromCapturePath: '$.cases.rejectedUpdateAboveLimit.variables.webhookSubscription',
              },
            },
          },
        },
        {
          name: 'read-after-rejected-filter-update',
          capturePath: '$.cases.detailAfterRejectedUpdate.response.payload.data.webhookSubscription',
          proxyPath: '$.data.webhookSubscription',
          proxyRequest: {
            documentPath: detailRequestPath,
            variables: {
              id: {
                fromProxyResponse: 'create-update-base',
                path: '$.data.webhookSubscriptionCreate.webhookSubscription.id',
              },
            },
          },
          expectedDifferences: detailExpectedDifferences(),
        },
      ],
    },
  };
}
