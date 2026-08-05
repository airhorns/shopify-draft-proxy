/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'delivery-customization-function-reference-validation';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const fixturePath = path.join(fixtureDir, `${scenarioId}.json`);
const requestDir = path.join('config', 'parity-requests', 'shipping-fulfillments');
const requestPath = path.join(requestDir, `${scenarioId}.graphql`);
const specDir = path.join('config', 'parity-specs', 'shipping-fulfillments');
const specPath = path.join(specDir, `${scenarioId}.json`);

function readRecord(value: unknown): JsonRecord {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : {};
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function assertTransport(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function functionNodes(result: ConformanceGraphqlResult): JsonRecord[] {
  const data = readRecord(result.payload.data);
  const connection = readRecord(data['shopifyFunctions']);
  return readArray(connection['nodes']).map(readRecord);
}

function customizationNodes(result: ConformanceGraphqlResult): JsonRecord[] {
  const data = readRecord(result.payload.data);
  const connection = readRecord(data['deliveryCustomizations']);
  return readArray(connection['nodes']).map(readRecord);
}

function appIdTail(appId: string): string {
  return appId.split('/').at(-1) ?? appId;
}

function assertUserError(
  operation: ConformanceGraphqlResult,
  responseKey: string,
  expected: { field: string[]; code: string; message: string },
): void {
  assertTransport(operation, responseKey);
  const data = readRecord(operation.payload.data);
  const payload = readRecord(data[responseKey]);
  if (payload['deliveryCustomization'] !== null) {
    throw new Error(`${responseKey} unexpectedly created a delivery customization.`);
  }
  const errors = readArray(payload['userErrors']).map(readRecord);
  if (
    errors.length !== 1 ||
    errors[0]?.['code'] !== expected.code ||
    errors[0]?.['message'] !== expected.message ||
    JSON.stringify(errors[0]?.['field']) !== JSON.stringify(expected.field)
  ) {
    throw new Error(`${responseKey} userError mismatch: ${JSON.stringify({ expected, actual: errors }, null, 2)}`);
  }
}

const capabilityDocument = `query DeliveryCustomizationFunctionCapability {
  currentAppInstallation {
    app { id handle apiKey }
    accessScopes { handle }
  }
  shopifyFunctions(first: 100) {
    nodes {
      id
      title
      apiType
      description
      appKey
      app { __typename id title handle apiKey }
    }
  }
  deliveryCustomizations(first: 26) {
    nodes {
      id
      title
      enabled
      functionId
      shopifyFunction {
        id
        title
        apiType
        description
        appKey
        app { __typename id title handle apiKey }
      }
    }
    pageInfo { hasNextPage endCursor }
  }
}
`;

const invalidReferencesDocument = `mutation DeliveryCustomizationInvalidFunctionReferences(
  $missingId: DeliveryCustomizationInput!
  $wrongTypeId: DeliveryCustomizationInput!
  $missingHandle: DeliveryCustomizationInput!
  $wrongTypeHandle: DeliveryCustomizationInput!
) {
  missingId: deliveryCustomizationCreate(deliveryCustomization: $missingId) {
    deliveryCustomization { id title enabled functionId }
    userErrors { field message code }
  }
  wrongTypeId: deliveryCustomizationCreate(deliveryCustomization: $wrongTypeId) {
    deliveryCustomization { id title enabled functionId }
    userErrors { field message code }
  }
  missingHandle: deliveryCustomizationCreate(deliveryCustomization: $missingHandle) {
    deliveryCustomization { id title enabled functionId }
    userErrors { field message code }
  }
  wrongTypeHandle: deliveryCustomizationCreate(deliveryCustomization: $wrongTypeHandle) {
    deliveryCustomization { id title enabled functionId }
    userErrors { field message code }
  }
}
`;

const hydrateByIdDocument = `query DeliveryCustomizationFunctionHydrateById($id: String!) {
  shopifyFunction(id: $id) {
    id
    title
    apiType
    description
    appKey
    app { __typename id title handle apiKey }
  }
}
`;

const hydrateFunctionCatalogDocument = `query DeliveryCustomizationFunctionCatalogHydrate {
  shopifyFunctions(first: 100) {
    nodes {
      id
      title
      apiType
      description
      appKey
      app { __typename id title handle apiKey }
    }
  }
}
`;

const capability = await runGraphqlRequest(capabilityDocument, {});
assertTransport(capability, 'delivery customization capability probe');
const capabilityData = readRecord(capability.payload.data);
const installation = readRecord(capabilityData['currentAppInstallation']);
const currentApp = readRecord(installation['app']);
const currentAppId = readString(currentApp['id']);
if (!currentAppId) {
  throw new Error('The capability probe did not return the current app id.');
}
const currentAppClientId = appIdTail(currentAppId);
const scopes = readArray(installation['accessScopes'])
  .map(readRecord)
  .flatMap((scope) => (readString(scope['handle']) ? [readString(scope['handle']) as string] : []));
for (const requiredScope of ['read_delivery_customizations', 'write_delivery_customizations']) {
  if (!scopes.includes(requiredScope)) {
    throw new Error(`The current installation is missing required scope ${requiredScope}.`);
  }
}

const functions = functionNodes(capability);
const deliveryFunctions = functions.filter((node) => {
  const apiType = readString(node['apiType'])?.toLowerCase();
  return apiType === 'delivery_customization' || apiType === 'delivery_customization_legacy';
});
if (deliveryFunctions.length > 0) {
  throw new Error(
    `This capture records the current missing-delivery-Function capability blocker, but eligible Functions are now visible: ${JSON.stringify(deliveryFunctions, null, 2)}`,
  );
}
const existingCustomizations = customizationNodes(capability);
if (existingCustomizations.length > 0) {
  throw new Error(
    `This capture records the current cold-target blocker, but delivery customizations now exist: ${JSON.stringify(existingCustomizations, null, 2)}`,
  );
}
const wrongTypeFunction = functions.find((node) => readString(node['apiType']) === 'payment_customization');
const wrongTypeFunctionId = readString(wrongTypeFunction?.['id']);
const wrongTypeFunctionHandle = readString(wrongTypeFunction?.['title']);
if (!wrongTypeFunctionId || !wrongTypeFunctionHandle) {
  throw new Error('No current-app payment_customization Function is available for the wrong-type probes.');
}

const missingFunctionId = '00000000-0000-0000-0000-000000000000';
const missingFunctionHandle = 'missing-delivery-customization-function';
const input = (title: string): JsonRecord => ({ title, enabled: false, metafields: [] });
const invalidVariables = {
  missingId: { ...input('Missing delivery Function id'), functionId: missingFunctionId },
  wrongTypeId: { ...input('Wrong-type delivery Function id'), functionId: wrongTypeFunctionId },
  missingHandle: { ...input('Missing delivery Function handle'), functionHandle: missingFunctionHandle },
  wrongTypeHandle: {
    ...input('Wrong-type delivery Function handle'),
    functionHandle: wrongTypeFunctionHandle,
  },
};
const invalidReferences = await runGraphqlRequest(invalidReferencesDocument, invalidVariables);
const notFoundMessage = (reference: string): string =>
  `Function ${reference} not found. Ensure that it is released in the current app (${currentAppClientId}), and that the app is installed.`;
const wrongTypeMessage =
  'Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.delivery-customization.run, cart.delivery-options.transform.run].';
assertUserError(invalidReferences, 'missingId', {
  field: ['deliveryCustomization', 'functionId'],
  code: 'FUNCTION_NOT_FOUND',
  message: notFoundMessage(missingFunctionId),
});
assertUserError(invalidReferences, 'wrongTypeId', {
  field: ['deliveryCustomization', 'functionId'],
  code: 'FUNCTION_DOES_NOT_IMPLEMENT',
  message: wrongTypeMessage,
});
assertUserError(invalidReferences, 'missingHandle', {
  field: ['deliveryCustomization', 'functionHandle'],
  code: 'FUNCTION_NOT_FOUND',
  message: notFoundMessage(missingFunctionHandle),
});
assertUserError(invalidReferences, 'wrongTypeHandle', {
  field: ['deliveryCustomization', 'functionHandle'],
  code: 'FUNCTION_DOES_NOT_IMPLEMENT',
  message: wrongTypeMessage,
});

const hydrateCalls: Array<{
  operationName: string;
  variables: JsonRecord;
  query: string;
  response: { status: number; body: unknown };
}> = [];
for (const id of [missingFunctionId, wrongTypeFunctionId]) {
  const response = await runGraphqlRequest(hydrateByIdDocument, { id });
  assertTransport(response, `delivery Function hydrate by id ${id}`);
  hydrateCalls.push({
    operationName: 'DeliveryCustomizationFunctionHydrateById',
    variables: { id },
    query: hydrateByIdDocument,
    response: { status: response.status, body: response.payload },
  });
}
const functionCatalogHydrate = await runGraphqlRequest(hydrateFunctionCatalogDocument, {});
assertTransport(functionCatalogHydrate, 'delivery Function catalog hydrate for handle resolution');
hydrateCalls.push({
  operationName: 'DeliveryCustomizationFunctionCatalogHydrate',
  variables: {},
  query: hydrateFunctionCatalogDocument,
  response: { status: functionCatalogHydrate.status, body: functionCatalogHydrate.payload },
});

const fixture = {
  scenarioId,
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  capability: {
    query: capabilityDocument,
    variables: {},
    response: capability.payload,
    blocker: {
      currentAppId,
      requiredScopesPresent: ['read_delivery_customizations', 'write_delivery_customizations'],
      eligibleDeliveryFunctionCount: deliveryFunctions.length,
      existingDeliveryCustomizationCount: existingCustomizations.length,
      blockedCaptureBranches: [
        'valid Function reference',
        'wrong-app Function reference',
        'cold existing customization target',
        '25-active-customization limit',
      ],
    },
  },
  operations: {
    invalidReferences: {
      query: invalidReferencesDocument,
      variables: invalidVariables,
      response: invalidReferences.payload,
    },
  },
  upstreamCalls: hydrateCalls,
  notes:
    'Live Shopify 2026-04 invalid-reference evidence plus the exact current capability blocker: the installed app has both delivery-customization scopes, but exposes no eligible delivery-customization Function and the shop has no existing delivery customizations. Valid, wrong-app, cold-target, and active-limit capture therefore require the delivery Function prerequisite.',
};

const paritySpec = {
  scenarioId,
  operationNames: ['deliveryCustomizationCreate'],
  scenarioStatus: 'captured',
  assertionKinds: ['payload-shape', 'user-errors-parity', 'no-upstream-passthrough'],
  liveCaptureFiles: [fixturePath],
  runtimeTestFiles: ['tests/graphql_routes/admin_app_shipping.rs'],
  comparisonMode: 'captured-vs-proxy-request',
  proxyRequest: {
    documentPath: requestPath,
    variablesCapturePath: '$.operations.invalidReferences.variables',
    headers: { 'x-shopify-draft-proxy-api-client-id': currentAppClientId },
  },
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [],
    targets: [
      {
        name: 'missing-function-id-error',
        capturePath: '$.operations.invalidReferences.response.data.missingId',
        proxyPath: '$.data.missingId',
      },
      {
        name: 'wrong-type-function-id-error',
        capturePath: '$.operations.invalidReferences.response.data.wrongTypeId',
        proxyPath: '$.data.wrongTypeId',
      },
      {
        name: 'missing-function-handle-error',
        capturePath: '$.operations.invalidReferences.response.data.missingHandle',
        proxyPath: '$.data.missingHandle',
      },
      {
        name: 'wrong-type-function-handle-error',
        capturePath: '$.operations.invalidReferences.response.data.wrongTypeHandle',
        proxyPath: '$.data.wrongTypeHandle',
      },
    ],
  },
  notes:
    'Executable invalid Function-reference parity. The live fixture also records the exact installed-app capability blocker for valid, wrong-app, cold-target, and active-limit delivery-customization capture.',
};

await Promise.all([
  mkdir(fixtureDir, { recursive: true }),
  mkdir(requestDir, { recursive: true }),
  mkdir(specDir, { recursive: true }),
]);
await Promise.all([
  writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8'),
  writeFile(requestPath, invalidReferencesDocument, 'utf8'),
  writeFile(specPath, `${JSON.stringify(paritySpec, null, 2)}\n`, 'utf8'),
]);
console.log(`Wrote ${fixturePath}`);
console.log(`Wrote ${requestPath}`);
console.log(`Wrote ${specPath}`);
