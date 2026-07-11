/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CapturedRequest = {
  documentPath: string;
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
};

type RecordedUpstreamCall = {
  operationName: string;
  query: string;
  variables: JsonRecord;
  response: {
    status: number;
    body: ConformanceGraphqlPayload;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, 'shipping-service-mutation-first-hydration.json');

const carrierCreateDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'carrier-service-lifecycle-create.graphql',
);
const carrierUpdateDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'carrier-service-lifecycle-update.graphql',
);
const carrierDeleteDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'carrier-service-lifecycle-delete.graphql',
);
const carrierDuplicateCreateDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'carrier-service-create-uniqueness.graphql',
);
const fulfillmentCreateDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-lifecycle-create.graphql',
);
const fulfillmentUpdateDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-lifecycle-update.graphql',
);
const fulfillmentDeleteDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-lifecycle-delete.graphql',
);
const fulfillmentDuplicateCreateDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-uniqueness-create.graphql',
);

const CARRIER_SERVICE_HYDRATE_QUERY = `query ShippingCarrierServiceHydrate($id: ID!) {
  carrierService(id: $id) {
    id
    name
    formattedName
    callbackUrl
    active
    supportsServiceDiscovery
  }
}`;
const CARRIER_SERVICES_HYDRATE_QUERY = `query ShippingCarrierServicesHydrate {
  carrierServices(first: 250) {
    nodes {
      id
      name
      formattedName
      callbackUrl
      active
      supportsServiceDiscovery
    }
  }
}`;
const FULFILLMENT_SERVICE_HYDRATE_QUERY = `query ShippingFulfillmentServiceHydrate($id: ID!) {
  fulfillmentService(id: $id) {
    id
    handle
    serviceName
    callbackUrl
    trackingSupport
    inventoryManagement
    requiresShippingMethod
    type
    location {
      id
      name
      isFulfillmentService
      fulfillsOnlineOrders
      shipsInventory
    }
  }
}`;
const FULFILLMENT_SERVICES_HYDRATE_QUERY = `query ShippingFulfillmentServicesHydrate {
  shop {
    fulfillmentServices {
      id
      handle
      serviceName
      callbackUrl
      trackingSupport
      inventoryManagement
      requiresShippingMethod
      type
      location {
        id
        name
        isFulfillmentService
        fulfillsOnlineOrders
        shipsInventory
      }
    }
  }
}`;

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

async function readRequest(documentPath: string): Promise<string> {
  return await readFile(path.join(process.cwd(), documentPath), 'utf8');
}

async function capture(documentPath: string, variables: JsonRecord): Promise<CapturedRequest> {
  const query = await readRequest(documentPath);
  return {
    documentPath,
    query,
    variables,
    response: await client.runGraphqlRequest(query, variables),
  };
}

async function captureUpstream(
  operationName: string,
  query: string,
  variables: JsonRecord = {},
): Promise<RecordedUpstreamCall> {
  const response = await client.runGraphqlRequest(query, variables);
  assertNoTopLevelErrors(response, `${operationName} hydrate`);
  return {
    operationName,
    query,
    variables,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function mutationPayload(result: ConformanceGraphqlResult, root: string, context: string): JsonRecord {
  const data = readObject(result.payload.data);
  const payload = readObject(data?.[root]);
  if (!payload) {
    throw new Error(`${context} expected ${root} payload: ${JSON.stringify(result.payload)}`);
  }
  return payload;
}

function assertNoUserErrors(result: ConformanceGraphqlResult, root: string, context: string): void {
  assertNoTopLevelErrors(result, context);
  const payload = mutationPayload(result, root, context);
  const userErrors = payload['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors)}`);
}

function assertCarrierDuplicateUserError(result: ConformanceGraphqlResult, expectedMessage: string): void {
  assertNoTopLevelErrors(result, 'carrier duplicate create');
  const payload = mutationPayload(result, 'carrierServiceCreate', 'carrier duplicate create');
  if (payload['carrierService'] !== null) {
    throw new Error(`carrier duplicate create expected null carrierService: ${JSON.stringify(payload)}`);
  }
  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 1) {
    throw new Error(`carrier duplicate create expected one userError: ${JSON.stringify(payload)}`);
  }
  const error = readObject(userErrors[0]);
  if (
    error?.['field'] !== null ||
    error?.['message'] !== expectedMessage ||
    error?.['code'] !== 'CARRIER_SERVICE_CREATE_FAILED'
  ) {
    throw new Error(`carrier duplicate create unexpected userError: ${JSON.stringify(error)}`);
  }
}

function assertFulfillmentDuplicateUserError(result: ConformanceGraphqlResult): void {
  assertNoTopLevelErrors(result, 'fulfillment duplicate create');
  const payload = mutationPayload(result, 'fulfillmentServiceCreate', 'fulfillment duplicate create');
  if (payload['fulfillmentService'] !== null) {
    throw new Error(`fulfillment duplicate create expected null fulfillmentService: ${JSON.stringify(payload)}`);
  }
  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 1) {
    throw new Error(`fulfillment duplicate create expected one userError: ${JSON.stringify(payload)}`);
  }
  const error = readObject(userErrors[0]);
  const field = error ? error['field'] : null;
  if (
    !Array.isArray(field) ||
    field.length !== 1 ||
    field[0] !== 'name' ||
    error?.['message'] !== 'Name has already been taken'
  ) {
    throw new Error(`fulfillment duplicate create unexpected userError: ${JSON.stringify(error)}`);
  }
}

function readCarrierServiceId(result: ConformanceGraphqlResult, context: string): string {
  const payload = mutationPayload(result, 'carrierServiceCreate', context);
  const carrier = readObject(payload['carrierService']);
  const id = carrier?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`${context} expected carrier service id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function readFulfillmentServiceId(result: ConformanceGraphqlResult, context: string): string {
  const payload = mutationPayload(result, 'fulfillmentServiceCreate', context);
  const service = readObject(payload['fulfillmentService']);
  const id = service?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`${context} expected fulfillment service id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

async function cleanupCarrierService(id: string, deleteDocument: string): Promise<ConformanceGraphqlResult | null> {
  try {
    return await client.runGraphqlRequest(deleteDocument, { id });
  } catch (error) {
    console.error(`Failed to cleanup carrier service ${id}:`, error);
    return null;
  }
}

async function cleanupFulfillmentService(id: string, deleteDocument: string): Promise<ConformanceGraphqlResult | null> {
  try {
    return await client.runGraphqlRequest(deleteDocument, { id });
  } catch (error) {
    console.error(`Failed to cleanup fulfillment service ${id}:`, error);
    return null;
  }
}

const carrierDeleteDocument = await readRequest(carrierDeleteDocumentPath);
const fulfillmentDeleteDocument = await readRequest(fulfillmentDeleteDocumentPath);
const suffix = Date.now().toString(36);

const carrierUpdateSetupVariables = {
  input: {
    name: `Hydrate Carrier Update ${suffix}`,
    callbackUrl: 'https://mock.shop/carrier-hydrate-update',
    supportsServiceDiscovery: true,
    active: false,
  },
};
const carrierDeleteSetupVariables = {
  input: {
    name: `Hydrate Carrier Delete ${suffix}`,
    callbackUrl: 'https://mock.shop/carrier-hydrate-delete',
    supportsServiceDiscovery: false,
    active: false,
  },
};
const carrierDuplicateSetupVariables = {
  input: {
    name: `Hydrate Carrier Duplicate ${suffix}`,
    callbackUrl: 'https://mock.shop/carrier-hydrate-duplicate',
    supportsServiceDiscovery: false,
    active: true,
  },
};
const fulfillmentUpdateSetupVariables = {
  name: `Hydrate FS Update ${suffix}`,
};
const fulfillmentDeleteSetupVariables = {
  name: `Hydrate FS Delete ${suffix}`,
};
const fulfillmentDuplicateSetupVariables = {
  name: `Hydrate FS Duplicate ${suffix}`,
};

const cleanupCarrierIds = new Set<string>();
const cleanupFulfillmentIds = new Set<string>();
const cleanup: JsonRecord = {
  carrierServices: [],
  fulfillmentServices: [],
};
const upstreamCalls: RecordedUpstreamCall[] = [];

let carrierUpdateSetup: CapturedRequest | null = null;
let carrierDeleteSetup: CapturedRequest | null = null;
let carrierDuplicateSetup: CapturedRequest | null = null;
let fulfillmentUpdateSetup: CapturedRequest | null = null;
let fulfillmentDeleteSetup: CapturedRequest | null = null;
let fulfillmentDuplicateSetup: CapturedRequest | null = null;
let carrierUpdate: CapturedRequest | null = null;
let carrierDelete: CapturedRequest | null = null;
let carrierDuplicateCreate: CapturedRequest | null = null;
let fulfillmentDelete: CapturedRequest | null = null;
let fulfillmentUpdate: CapturedRequest | null = null;
let fulfillmentDuplicateCreate: CapturedRequest | null = null;

try {
  carrierUpdateSetup = await capture(carrierCreateDocumentPath, carrierUpdateSetupVariables);
  assertNoUserErrors(carrierUpdateSetup.response, 'carrierServiceCreate', 'carrier update setup');
  const carrierUpdateId = readCarrierServiceId(carrierUpdateSetup.response, 'carrier update setup');
  cleanupCarrierIds.add(carrierUpdateId);

  carrierDeleteSetup = await capture(carrierCreateDocumentPath, carrierDeleteSetupVariables);
  assertNoUserErrors(carrierDeleteSetup.response, 'carrierServiceCreate', 'carrier delete setup');
  const carrierDeleteId = readCarrierServiceId(carrierDeleteSetup.response, 'carrier delete setup');
  cleanupCarrierIds.add(carrierDeleteId);

  fulfillmentUpdateSetup = await capture(fulfillmentCreateDocumentPath, fulfillmentUpdateSetupVariables);
  assertNoUserErrors(fulfillmentUpdateSetup.response, 'fulfillmentServiceCreate', 'fulfillment update setup');
  const fulfillmentUpdateId = readFulfillmentServiceId(fulfillmentUpdateSetup.response, 'fulfillment update setup');
  cleanupFulfillmentIds.add(fulfillmentUpdateId);

  fulfillmentDeleteSetup = await capture(fulfillmentCreateDocumentPath, fulfillmentDeleteSetupVariables);
  assertNoUserErrors(fulfillmentDeleteSetup.response, 'fulfillmentServiceCreate', 'fulfillment delete setup');
  const fulfillmentDeleteId = readFulfillmentServiceId(fulfillmentDeleteSetup.response, 'fulfillment delete setup');
  cleanupFulfillmentIds.add(fulfillmentDeleteId);

  fulfillmentDuplicateSetup = await capture(fulfillmentCreateDocumentPath, fulfillmentDuplicateSetupVariables);
  assertNoUserErrors(fulfillmentDuplicateSetup.response, 'fulfillmentServiceCreate', 'fulfillment duplicate setup');
  const fulfillmentDuplicateId = readFulfillmentServiceId(
    fulfillmentDuplicateSetup.response,
    'fulfillment duplicate setup',
  );
  cleanupFulfillmentIds.add(fulfillmentDuplicateId);

  upstreamCalls.push(
    await captureUpstream('ShippingCarrierServiceHydrate', CARRIER_SERVICE_HYDRATE_QUERY, {
      id: carrierUpdateId,
    }),
  );
  carrierUpdate = await capture(carrierUpdateDocumentPath, {
    input: {
      id: carrierUpdateId,
      name: `Hydrate Carrier Updated ${suffix}`,
      callbackUrl: 'https://mock.shop/carrier-hydrate-updated',
      supportsServiceDiscovery: false,
      active: false,
    },
  });
  assertNoUserErrors(carrierUpdate.response, 'carrierServiceUpdate', 'carrier mutation-first update');

  upstreamCalls.push(
    await captureUpstream('ShippingCarrierServiceHydrate', CARRIER_SERVICE_HYDRATE_QUERY, {
      id: carrierDeleteId,
    }),
  );
  carrierDelete = await capture(carrierDeleteDocumentPath, { id: carrierDeleteId });
  assertNoUserErrors(carrierDelete.response, 'carrierServiceDelete', 'carrier mutation-first delete');
  cleanupCarrierIds.delete(carrierDeleteId);

  carrierDuplicateSetup = await capture(carrierCreateDocumentPath, carrierDuplicateSetupVariables);
  assertNoUserErrors(carrierDuplicateSetup.response, 'carrierServiceCreate', 'carrier duplicate setup');
  const carrierDuplicateId = readCarrierServiceId(carrierDuplicateSetup.response, 'carrier duplicate setup');
  cleanupCarrierIds.add(carrierDuplicateId);

  upstreamCalls.push(await captureUpstream('ShippingCarrierServicesHydrate', CARRIER_SERVICES_HYDRATE_QUERY));
  carrierDuplicateCreate = await capture(carrierDuplicateCreateDocumentPath, {
    input: {
      name: carrierDuplicateSetupVariables.input.name,
      callbackUrl: 'https://mock.shop/carrier-hydrate-duplicate-new',
      supportsServiceDiscovery: false,
      active: true,
    },
  });
  assertCarrierDuplicateUserError(
    carrierDuplicateCreate.response,
    `${carrierDuplicateSetupVariables.input.name} is already configured`,
  );

  upstreamCalls.push(
    await captureUpstream('ShippingFulfillmentServiceHydrate', FULFILLMENT_SERVICE_HYDRATE_QUERY, {
      id: fulfillmentDeleteId,
    }),
  );
  fulfillmentDelete = await capture(fulfillmentDeleteDocumentPath, { id: fulfillmentDeleteId });
  assertNoUserErrors(fulfillmentDelete.response, 'fulfillmentServiceDelete', 'fulfillment mutation-first delete');
  cleanupFulfillmentIds.delete(fulfillmentDeleteId);

  upstreamCalls.push(
    await captureUpstream('ShippingFulfillmentServiceHydrate', FULFILLMENT_SERVICE_HYDRATE_QUERY, {
      id: fulfillmentUpdateId,
    }),
  );
  upstreamCalls.push(await captureUpstream('ShippingFulfillmentServicesHydrate', FULFILLMENT_SERVICES_HYDRATE_QUERY));
  fulfillmentUpdate = await capture(fulfillmentUpdateDocumentPath, {
    id: fulfillmentUpdateId,
    name: `Hydrate FS Updated ${suffix}`,
  });
  assertNoUserErrors(fulfillmentUpdate.response, 'fulfillmentServiceUpdate', 'fulfillment mutation-first update');

  upstreamCalls.push(await captureUpstream('ShippingFulfillmentServicesHydrate', FULFILLMENT_SERVICES_HYDRATE_QUERY));
  fulfillmentDuplicateCreate = await capture(fulfillmentDuplicateCreateDocumentPath, {
    name: fulfillmentDuplicateSetupVariables.name,
  });
  assertFulfillmentDuplicateUserError(fulfillmentDuplicateCreate.response);
} finally {
  for (const id of [...cleanupCarrierIds].reverse()) {
    const result = await cleanupCarrierService(id, carrierDeleteDocument);
    (cleanup['carrierServices'] as unknown[]).push({ id, result });
  }
  for (const id of [...cleanupFulfillmentIds].reverse()) {
    const result = await cleanupFulfillmentService(id, fulfillmentDeleteDocument);
    (cleanup['fulfillmentServices'] as unknown[]).push({ id, result });
  }
}

for (const [name, value] of Object.entries({
  carrierUpdateSetup,
  carrierDeleteSetup,
  carrierDuplicateSetup,
  fulfillmentUpdateSetup,
  fulfillmentDeleteSetup,
  fulfillmentDuplicateSetup,
  carrierUpdate,
  carrierDelete,
  carrierDuplicateCreate,
  fulfillmentDelete,
  fulfillmentUpdate,
  fulfillmentDuplicateCreate,
})) {
  if (value === null) {
    throw new Error(`Expected capture ${name} to be present.`);
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      suffix,
      notes: [
        'Live Admin GraphQL capture for mutation-first carrier-service and fulfillment-service hydration.',
        'Setup creates disposable carrier and fulfillment services through the current conformance app, so mutation targets are app-owned and valid for lifecycle updates/deletes.',
        'Carrier services use mock.shop callback URLs accepted by the conformance app; fulfillment services are created without callback URLs and therefore create Shopify-managed fulfillment-service locations.',
        'The upstreamCalls cassette contains only exact GraphQL query documents and variables used by the proxy hydration path; lifecycle mutations remain local during parity replay and are replayable later through commit.',
        'Cleanup deletes the disposable carrier services and fulfillment services that were not already deleted by the captured lifecycle mutations.',
      ],
      setup: {
        carrierUpdateSetup,
        carrierDeleteSetup,
        carrierDuplicateSetup,
        fulfillmentUpdateSetup,
        fulfillmentDeleteSetup,
        fulfillmentDuplicateSetup,
      },
      carrierUpdate,
      carrierDelete,
      carrierDuplicateCreate,
      fulfillmentDelete,
      fulfillmentUpdate,
      fulfillmentDuplicateCreate,
      cleanup,
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, outputPath }, null, 2));
