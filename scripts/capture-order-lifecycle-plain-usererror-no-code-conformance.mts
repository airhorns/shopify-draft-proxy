import { readFile } from 'node:fs/promises';

import { createConformanceCapture, type JsonRecord } from './conformance-capture-lib.js';

const scenarioId = 'orderLifecycle-plain-usererror-no-code';
const domain = 'orders';
const requestFile = `${scenarioId}.graphql`;
const variablesPath = `config/parity-requests/${domain}/${scenarioId}.variables.json`;
const specPath = `config/parity-specs/${domain}/${scenarioId}.json`;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function assertUndefinedFieldErrors(payload: JsonRecord): void {
  const errors = payload['errors'];
  if (!Array.isArray(errors) || errors.length !== 4) {
    throw new Error(`Expected four top-level GraphQL errors: ${JSON.stringify(payload, null, 2)}`);
  }
  if ('data' in payload && payload['data'] !== null) {
    throw new Error(`Expected no data or null data for schema validation failure: ${JSON.stringify(payload, null, 2)}`);
  }
  const expectedResponseKeys = ['update', 'close', 'open', 'markAsPaid'];
  for (const [index, error] of errors.entries()) {
    if (!isRecord(error)) {
      throw new Error(`Expected error object: ${JSON.stringify(error)}`);
    }
    const extensions = error['extensions'];
    const path = error['path'];
    if (
      error['message'] !== "Field 'code' doesn't exist on type 'UserError'" ||
      !Array.isArray(error['locations']) ||
      !Array.isArray(path) ||
      path[1] !== expectedResponseKeys[index] ||
      !isRecord(extensions) ||
      extensions['code'] !== 'undefinedField' ||
      extensions['typeName'] !== 'UserError' ||
      extensions['fieldName'] !== 'code'
    ) {
      throw new Error(`Unexpected UserError.code validation payload: ${JSON.stringify(error, null, 2)}`);
    }
  }
}

function buildSpec(apiVersion: string, fixturePath: string): JsonRecord {
  return {
    scenarioId,
    operationNames: ['orderUpdate', 'orderClose', 'orderOpen', 'orderMarkAsPaid'],
    scenarioStatus: 'captured',
    assertionKinds: ['graphql-validation-parity', 'schema-validation', 'user-errors-parity'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes/orders.rs'],
    proxyRequest: {
      documentPath: `config/parity-requests/${domain}/${requestFile}`,
      variablesCapturePath: '$.codeSelection.request.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'order-lifecycle-user-errors-reject-code-selection',
          capturePath: '$.codeSelection.response.payload',
          proxyPath: '$',
        },
      ],
    },
    notes:
      'Live Admin GraphQL 2026-04 capture proves orderUpdate, orderClose, orderOpen, and orderMarkAsPaid expose plain UserError payloads for userErrors. Selecting userErrors.code is rejected by GraphQL validation with undefinedField before any order lifecycle resolver can mutate state.',
  };
}

const capture = await createConformanceCapture();
const query = await capture.readRequestRaw(domain, requestFile);
const variables = JSON.parse(await readFile(variablesPath, 'utf8')) as JsonRecord;
const response = await capture.runGraphqlRequest(query, variables);
if (response.status < 200 || response.status >= 300 || !isRecord(response.payload)) {
  throw new Error(`Shopify GraphQL request failed: ${JSON.stringify(response, null, 2)}`);
}
assertUndefinedFieldErrors(response.payload);

const fixturePath = capture.fixturePath(domain, `${scenarioId}.json`);
await capture.writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  storeDomain: capture.storeDomain,
  apiVersion: capture.apiVersion,
  liveGatewaySideEffects: false,
  rootAvailability: {
    mutations: ['orderClose', 'orderMarkAsPaid', 'orderOpen', 'orderUpdate'],
  },
  notes:
    'Validation-only GraphQL request. Selecting userErrors.code fails before resolver execution and does not create or mutate Shopify resources.',
  codeSelection: {
    query,
    request: { variables },
    response,
  },
  upstreamCalls: [],
});
await capture.writeJson(specPath, buildSpec(capture.apiVersion, fixturePath));
