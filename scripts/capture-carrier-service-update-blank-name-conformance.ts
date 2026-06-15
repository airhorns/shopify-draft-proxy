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
const outputPath = path.join(outputDir, 'carrier-service-update-blank-name.json');

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'shipping-fulfillments', name), 'utf8');
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function rootPayload(payload: ConformanceGraphqlPayload, root: string): JsonRecord {
  const data = readObject(payload.data);
  const fieldPayload = readObject(data?.[root]);
  if (!fieldPayload) {
    throw new Error(`Expected ${root} payload: ${JSON.stringify(payload)}`);
  }

  return fieldPayload;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(result: ConformanceGraphqlResult, root: string, context: string): void {
  assertNoTopLevelErrors(result, context);
  const payload = rootPayload(result.payload, root);
  const userErrors = payload['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }
  throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors)}`);
}

function assertBlankNameUserError(result: ConformanceGraphqlResult): void {
  assertNoTopLevelErrors(result, 'blank-name update');
  const payload = rootPayload(result.payload, 'carrierServiceUpdate');
  if (payload['carrierService'] !== null) {
    throw new Error(`Expected blank-name update carrierService to be null: ${JSON.stringify(payload)}`);
  }
  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length === 0) {
    throw new Error(`Expected blank-name update userErrors: ${JSON.stringify(payload)}`);
  }
  const first = readObject(userErrors[0]);
  if (
    first?.['field'] !== null ||
    first?.['message'] !== "Shipping rate provider name can't be blank" ||
    first?.['code'] !== 'CARRIER_SERVICE_UPDATE_FAILED'
  ) {
    throw new Error(`Unexpected blank-name update userError: ${JSON.stringify(userErrors)}`);
  }
}

function readCreatedCarrierServiceId(result: ConformanceGraphqlResult): string {
  const payload = rootPayload(result.payload, 'carrierServiceCreate');
  const carrierService = readObject(payload['carrierService']);
  const id = carrierService?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`Expected carrierServiceCreate id: ${JSON.stringify(result.payload)}`);
  }

  return id;
}

function assertReadName(result: ConformanceGraphqlResult, expectedName: string): void {
  assertNoTopLevelErrors(result, 'after rejected update read');
  const data = readObject(result.payload.data);
  const carrierService = readObject(data?.['carrierService']);
  if (carrierService?.['name'] !== expectedName) {
    throw new Error(`Expected read-after-reject name ${expectedName}: ${JSON.stringify(result.payload)}`);
  }
}

async function cleanup(id: string, deleteDocument: string): Promise<void> {
  try {
    const result = await client.runGraphqlRequest(deleteDocument, { id });
    if (result.status < 200 || result.status >= 300 || result.payload.errors) {
      console.error(`Failed to cleanup carrier service ${id}: ${JSON.stringify(result.payload)}`);
    }
  } catch (error) {
    console.error(`Failed to cleanup carrier service ${id}:`, error);
  }
}

const createDocument = await readRequest('carrier-service-update-blank-name-create.graphql');
const updateDocument = await readRequest('carrier-service-update-blank-name.graphql');
const readDocument = await readRequest('carrier-service-update-blank-name-read.graphql');
const deleteDocument = await readRequest('carrier-service-lifecycle-delete.graphql');
const token = `csblank-${Date.now().toString(36)}`;
const createVariables = {
  input: {
    name: `Carrier Blank Update ${token}`,
    callbackUrl: 'https://mock.shop/carrier-service-rates',
    supportsServiceDiscovery: false,
    active: true,
  },
};
const cleanupIds: string[] = [];

let create: ConformanceGraphqlResult | null = null;
let blankNameUpdate: ConformanceGraphqlResult | null = null;
let afterRejectedUpdateRead: ConformanceGraphqlResult | null = null;

try {
  create = await client.runGraphqlRequest(createDocument, createVariables);
  assertNoUserErrors(create, 'carrierServiceCreate', 'carrier-service setup create');
  const carrierServiceId = readCreatedCarrierServiceId(create);
  cleanupIds.push(carrierServiceId);

  const blankNameUpdateVariables = {
    input: {
      id: carrierServiceId,
      name: '',
    },
  };
  blankNameUpdate = await client.runGraphqlRequest(updateDocument, blankNameUpdateVariables);
  assertBlankNameUserError(blankNameUpdate);

  const afterRejectedUpdateReadVariables = { id: carrierServiceId };
  afterRejectedUpdateRead = await client.runGraphqlRequest(readDocument, afterRejectedUpdateReadVariables);
  assertReadName(afterRejectedUpdateRead, createVariables.input.name);

  const fixture = {
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    token,
    notes: [
      'Live Admin GraphQL DeliveryCarrierService capture for carrierServiceUpdate with a present blank name.',
      'The setup create is disposable and exists only to target update validation.',
      'The blank-name update returns a typed carrier-service userError and leaves the existing carrier service name unchanged on a downstream read.',
      'The created carrier service is deleted during cleanup.',
    ],
    create: {
      documentPath: 'config/parity-requests/shipping-fulfillments/carrier-service-update-blank-name-create.graphql',
      variables: createVariables,
      response: create,
    },
    blankNameUpdate: {
      documentPath: 'config/parity-requests/shipping-fulfillments/carrier-service-update-blank-name.graphql',
      variables: blankNameUpdateVariables,
      response: blankNameUpdate,
    },
    afterRejectedUpdateRead: {
      documentPath: 'config/parity-requests/shipping-fulfillments/carrier-service-update-blank-name-read.graphql',
      variables: afterRejectedUpdateReadVariables,
      response: afterRejectedUpdateRead,
    },
    upstreamCalls: [],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath: outputPath }, null, 2));
} finally {
  for (const id of cleanupIds.reverse()) {
    await cleanup(id, deleteDocument);
  }
}
