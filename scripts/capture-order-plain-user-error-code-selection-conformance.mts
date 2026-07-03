import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'order-refund-fulfillment-usererror-no-code';
const responseKeys = ['refund', 'create', 'createV2', 'cancel', 'tracking', 'trackingV2', 'event'] as const;
const rootNames = [
  'refundCreate',
  'fulfillmentCreate',
  'fulfillmentCreateV2',
  'fulfillmentCancel',
  'fulfillmentTrackingInfoUpdate',
  'fulfillmentTrackingInfoUpdateV2',
  'fulfillmentEventCreate',
] as const;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const outputPath = path.join(outputDir, `${scenarioId}.json`);
const requestPath = path.join('config', 'parity-requests', 'orders', `${scenarioId}.graphql`);
const specPath = path.join('config', 'parity-specs', 'orders', `${scenarioId}.json`);

const codeSelectionMutation = `#graphql
  mutation OrdersPlainUserErrorCodeSelectionRejected {
    refund: refundCreate(input: { orderId: "gid://shopify/Order/999999999" }) {
      userErrors {
        field
        message
        code
      }
    }
    create: fulfillmentCreate(fulfillment: {
      lineItemsByFulfillmentOrder: [{ fulfillmentOrderId: "gid://shopify/FulfillmentOrder/1" }]
    }) {
      userErrors {
        code
      }
    }
    createV2: fulfillmentCreateV2(fulfillment: {
      lineItemsByFulfillmentOrder: [{ fulfillmentOrderId: "gid://shopify/FulfillmentOrder/1" }]
    }) {
      userErrors {
        code
      }
    }
    cancel: fulfillmentCancel(id: "gid://shopify/Fulfillment/1") {
      userErrors {
        code
      }
    }
    tracking: fulfillmentTrackingInfoUpdate(
      fulfillmentId: "gid://shopify/Fulfillment/1"
      trackingInfoInput: { number: "TRACK-1" }
    ) {
      userErrors {
        code
      }
    }
    trackingV2: fulfillmentTrackingInfoUpdateV2(
      fulfillmentId: "gid://shopify/Fulfillment/1"
      trackingInfoInput: { number: "TRACK-1" }
    ) {
      userErrors {
        code
      }
    }
    event: fulfillmentEventCreate(fulfillmentEvent: {
      fulfillmentId: "gid://shopify/Fulfillment/1"
      status: IN_TRANSIT
    }) {
      userErrors {
        code
      }
    }
  }
`;

const requestVariables: JsonRecord = {};
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const { status, payload } = await runGraphqlRequest(codeSelectionMutation, requestVariables);
if (status < 200 || status >= 300) {
  throw new Error(`Shopify GraphQL request failed with HTTP ${status}: ${JSON.stringify(payload)}`);
}
if (!isRecord(payload)) {
  throw new Error(`Shopify GraphQL response was not an object: ${JSON.stringify(payload)}`);
}
assertUndefinedFieldErrors(payload);

const cleanMutation = cleanGraphqlDocument(codeSelectionMutation);
const capture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  rootAvailability: {
    mutations: [...rootNames],
  },
  codeSelection: {
    query: codeSelectionMutation,
    request: { variables: requestVariables },
    response: { status, payload },
  },
  upstreamCalls: [],
};
const paritySpec = {
  scenarioId,
  operationNames: [...rootNames],
  scenarioStatus: 'captured',
  assertionKinds: ['graphql-validation-parity', 'mutation-validation'],
  liveCaptureFiles: [outputPath],
  runtimeTestFiles: ['tests/graphql_routes/orders.rs'],
  proxyRequest: {
    documentPath: requestPath,
    variablesCapturePath: '$.codeSelection.request.variables',
    apiVersion,
  },
  comparisonMode: 'captured-vs-proxy-request',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [],
    targets: [
      {
        name: 'order-refund-fulfillment-user-errors-reject-code-selection',
        capturePath: '$.codeSelection.response.payload.errors',
        proxyPath: '$.errors',
        selectedPaths: responseKeys.flatMap((_, index) => [
          `$[${index}].message`,
          `$[${index}].path`,
          `$[${index}].extensions`,
        ]),
      },
    ],
  },
  notes:
    'Live Admin GraphQL capture proves refundCreate, fulfillmentCreate, fulfillmentCreateV2, fulfillmentCancel, fulfillmentTrackingInfoUpdate, fulfillmentTrackingInfoUpdateV2, and fulfillmentEventCreate expose plain UserError payloads for userErrors. Selecting userErrors.code is rejected by GraphQL validation with undefinedField before any order, refund, or fulfillment resolver can mutate state.',
};

await Promise.all([
  mkdir(outputDir, { recursive: true }),
  mkdir(path.dirname(requestPath), { recursive: true }),
  mkdir(path.dirname(specPath), { recursive: true }),
]);
await Promise.all([
  writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8'),
  writeFile(requestPath, cleanMutation, 'utf8'),
  writeFile(specPath, `${JSON.stringify(paritySpec, null, 2)}\n`, 'utf8'),
]);

// oxlint-disable-next-line no-console -- CLI capture output is intentionally written to stdout.
console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      requestPath,
      specPath,
      storeDomain,
      apiVersion,
    },
    null,
    2,
  ),
);

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function cleanGraphqlDocument(document: string): string {
  return `${document
    .replace(/^#graphql\n/u, '')
    .trim()
    .replace(/^  /gmu, '')}\n`;
}

function assertUndefinedFieldErrors(payload: JsonRecord): void {
  const errors = payload['errors'];
  if (!Array.isArray(errors) || errors.length !== responseKeys.length) {
    throw new Error(`Expected seven top-level GraphQL errors: ${JSON.stringify(payload)}`);
  }
  if ('data' in payload && payload['data'] !== null) {
    throw new Error(`Expected no data for schema validation failure: ${JSON.stringify(payload)}`);
  }
  for (const [index, error] of errors.entries()) {
    if (!isRecord(error)) {
      throw new Error(`Expected error object: ${JSON.stringify(error)}`);
    }
    const extensions = error['extensions'];
    const expectedPath = [
      'mutation OrdersPlainUserErrorCodeSelectionRejected',
      responseKeys[index],
      'userErrors',
      'code',
    ];
    if (
      error['message'] !== "Field 'code' doesn't exist on type 'UserError'" ||
      JSON.stringify(error['path']) !== JSON.stringify(expectedPath) ||
      !isRecord(extensions) ||
      extensions['code'] !== 'undefinedField' ||
      extensions['typeName'] !== 'UserError' ||
      extensions['fieldName'] !== 'code'
    ) {
      throw new Error(`Unexpected UserError.code validation payload: ${JSON.stringify(error)}`);
    }
  }
}
