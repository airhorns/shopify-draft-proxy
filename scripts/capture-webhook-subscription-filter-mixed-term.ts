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

const scenarioId = 'webhook-subscription-filter-mixed-term';
const invalidFilterMessage =
  'The specified filter is invalid, please ensure you specify the field(s) you wish to filter on.';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'webhooks');
const outputPath = path.join(outputDir, `${scenarioId}.json`);
const specPath = path.join('config', 'parity-specs', 'webhooks', `${scenarioId}.json`);

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

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function requireSuccessfulGraphql(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result.payload)}`);
  }
}

async function captureDocument(
  documentPath: string,
  document: string,
  variables: Record<string, unknown>,
): Promise<CapturedRequest> {
  const response = await runGraphqlRequest(document, variables);
  requireSuccessfulGraphql(response, documentPath);

  return {
    documentPath,
    variables,
    response,
  };
}

async function capture(documentPath: string, variables: Record<string, unknown>): Promise<CapturedRequest> {
  return captureDocument(documentPath, await readText(documentPath), variables);
}

const customerCreateDocument = `#graphql
  mutation WebhookFilterMixedTermCustomerCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        id
        email
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const customerDeleteDocument = `#graphql
  mutation WebhookFilterMixedTermCustomerDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

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

function readCreatedCustomerId(captureResult: CapturedRequest): string {
  const payload = readPayload(captureResult, 'customerCreate');
  const customer = payload['customer'];
  if (!isObject(customer) || typeof customer['id'] !== 'string') {
    throw new Error(`Customer setup did not return a customer id: ${JSON.stringify(payload)}`);
  }

  return customer['id'];
}

function gidNumericTail(id: string): string {
  const tail = id.split('/').at(-1);
  return typeof tail === 'string' ? tail : id;
}

function createVariables(uri: string, filter: string): Record<string, unknown> {
  return {
    topic: 'CUSTOMERS_UPDATE',
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

function assertInvalidFilter(captureResult: CapturedRequest, root: string): void {
  const payload = readPayload(captureResult, root);
  if (payload['webhookSubscription'] !== null) {
    throw new Error(`Mixed filter unexpectedly created/updated a subscription: ${JSON.stringify(payload)}`);
  }

  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 1 || !isObject(userErrors[0])) {
    throw new Error(`Mixed filter returned malformed userErrors: ${JSON.stringify(payload)}`);
  }

  const error = userErrors[0];
  if (
    JSON.stringify(error['field']) !== JSON.stringify(['webhookSubscription']) ||
    error['message'] !== invalidFilterMessage
  ) {
    throw new Error(`Mixed filter returned unexpected userError: ${JSON.stringify(error)}`);
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
const createMixedUri = `https://example.com/hermes-webhook-filter-mixed-${suffix}-create`;
const qualifiedUri = `https://example.com/hermes-webhook-filter-mixed-${suffix}-qualified`;

let customerId: string | null = null;
let qualifiedId: string | null = null;
let cleanupQualified: CapturedRequest | null = null;
let cleanupCustomer: CapturedRequest | null = null;

const setupCustomer = await captureDocument(
  'inline:webhook-filter-mixed-term-customer-create',
  customerCreateDocument,
  {
    input: {
      email: `hermes-webhook-filter-mixed-${suffix}@example.com`,
      firstName: 'Hermes',
      lastName: 'WebhookFilterMixed',
    },
  },
);
customerId = readCreatedCustomerId(setupCustomer);
const qualifiedFilter = `customer_id:${gidNumericTail(customerId)}`;
const mixedFilter = `${qualifiedFilter} bareword`;

try {
  const createMixedQualifiedAndBareRejected = await capture(
    createRequestPath,
    createVariables(createMixedUri, mixedFilter),
  );
  assertInvalidFilter(createMixedQualifiedAndBareRejected, 'webhookSubscriptionCreate');

  const createFullyQualifiedAccepted = await capture(createRequestPath, createVariables(qualifiedUri, qualifiedFilter));
  qualifiedId = assertCreated(createFullyQualifiedAccepted, 'webhookSubscriptionCreate');

  const updateMixedQualifiedAndBareRejected = await capture(
    updateRequestPath,
    updateVariables(qualifiedId, qualifiedUri, mixedFilter),
  );
  assertInvalidFilter(updateMixedQualifiedAndBareRejected, 'webhookSubscriptionUpdate');

  const detailAfterRejectedUpdate = await capture(detailRequestPath, { id: qualifiedId });
  assertDetailFilter(detailAfterRejectedUpdate, qualifiedFilter);

  cleanupQualified = await capture(deleteRequestPath, { id: qualifiedId });
  cleanupCustomer = await captureDocument('inline:webhook-filter-mixed-term-customer-delete', customerDeleteDocument, {
    input: { id: customerId },
  });

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        notes: [
          'Captures Shopify webhook subscription filter syntax behavior for a mixed qualified field term plus bare/default term.',
          'The setup customer proves customer_id is a valid qualified filter for CUSTOMERS_UPDATE; adding a bareword rejects the whole filter.',
          'The rejected update capture plus detail read prove Shopify leaves the existing qualified filter unchanged.',
          'No webhook delivery is intentionally triggered by this validation capture.',
        ],
        setup: {
          customer: setupCustomer,
        },
        cases: {
          createMixedQualifiedAndBareRejected,
          createFullyQualifiedAccepted,
          updateMixedQualifiedAndBareRejected,
          detailAfterRejectedUpdate,
        },
        cleanup: {
          qualifiedSubscription: cleanupQualified,
          customer: cleanupCustomer,
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
  if (qualifiedId !== null && cleanupQualified === null) {
    await capture(deleteRequestPath, { id: qualifiedId });
  }
  if (customerId !== null && cleanupCustomer === null) {
    await captureDocument('inline:webhook-filter-mixed-term-customer-delete', customerDeleteDocument, {
      input: { id: customerId },
    });
  }
}

console.log(`Wrote webhook subscription mixed-filter fixture to ${outputPath}`);
console.log(`Wrote webhook subscription mixed-filter parity spec to ${specPath}`);

function webhookSubscriptionExpectedDifferences(): Record<string, unknown>[] {
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
      reason: 'The proxy uses its deterministic synthetic clock for locally staged webhook subscriptions.',
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
    scenarioId,
    operationNames: ['webhookSubscriptionCreate', 'webhookSubscriptionUpdate', 'webhookSubscription'],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity', 'payload-shape', 'local-staging', 'downstream-read-parity'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['tests/graphql_routes/admin_graphql_webhooks.rs'],
    proxyRequest: {
      documentPath: createRequestPath,
      apiVersion,
      variablesCapturePath: '$.cases.createMixedQualifiedAndBareRejected.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Strict parity for webhook subscription filter syntax when a qualified field term is mixed with a bare/default term. Shopify rejects the whole filter on create/update with field ["webhookSubscription"], while a fully qualified customer_id control remains accepted and survives the rejected update unchanged.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'create-mixed-qualified-and-bare-rejected',
          capturePath: '$.cases.createMixedQualifiedAndBareRejected.response.payload.data.webhookSubscriptionCreate',
          proxyPath: '$.data.webhookSubscriptionCreate',
        },
        {
          name: 'create-fully-qualified-accepted',
          capturePath: '$.cases.createFullyQualifiedAccepted.response.payload.data.webhookSubscriptionCreate',
          proxyPath: '$.data.webhookSubscriptionCreate',
          proxyRequest: {
            documentPath: createRequestPath,
            variablesCapturePath: '$.cases.createFullyQualifiedAccepted.variables',
          },
          expectedDifferences: webhookSubscriptionExpectedDifferences(),
        },
        {
          name: 'update-mixed-qualified-and-bare-rejected',
          capturePath: '$.cases.updateMixedQualifiedAndBareRejected.response.payload.data.webhookSubscriptionUpdate',
          proxyPath: '$.data.webhookSubscriptionUpdate',
          proxyRequest: {
            documentPath: updateRequestPath,
            variables: {
              id: {
                fromProxyResponse: 'create-fully-qualified-accepted',
                path: '$.data.webhookSubscriptionCreate.webhookSubscription.id',
              },
              webhookSubscription: {
                fromCapturePath: '$.cases.updateMixedQualifiedAndBareRejected.variables.webhookSubscription',
              },
            },
          },
        },
        {
          name: 'read-after-rejected-mixed-filter-update',
          capturePath: '$.cases.detailAfterRejectedUpdate.response.payload.data.webhookSubscription',
          proxyPath: '$.data.webhookSubscription',
          proxyRequest: {
            documentPath: detailRequestPath,
            variables: {
              id: {
                fromProxyResponse: 'create-fully-qualified-accepted',
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
