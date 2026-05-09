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
const outputPath = path.join(outputDir, 'fulfillment-service-name-whitespace-validation.json');

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'shipping-fulfillments', name), 'utf8');
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function mutationPayload(
  payload: ConformanceGraphqlPayload,
  root: 'createWhitespace' | 'setupUpdateSource' | 'fulfillmentServiceUpdate' | 'fulfillmentServiceCreate',
): JsonRecord {
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

function assertNoUserErrors(
  result: ConformanceGraphqlResult,
  root: 'setupUpdateSource' | 'fulfillmentServiceCreate',
  context: string,
): void {
  assertNoTopLevelErrors(result, context);
  const payload = mutationPayload(result.payload, root);
  const userErrors = payload['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }
  throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors)}`);
}

function assertNameUserErrors(
  result: ConformanceGraphqlResult,
  root: 'createWhitespace' | 'fulfillmentServiceUpdate',
  context: string,
): void {
  assertNoTopLevelErrors(result, context);
  const payload = mutationPayload(result.payload, root);
  if (payload['fulfillmentService'] !== null) {
    throw new Error(`Expected ${context} fulfillmentService to be null; got ${JSON.stringify(payload)}`);
  }
  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length === 0) {
    throw new Error(`Expected ${context} to return name userErrors; got ${JSON.stringify(userErrors)}.`);
  }
  for (const errorValue of userErrors) {
    const error = readObject(errorValue);
    const field = error?.['field'];
    if (!Array.isArray(field) || field.length !== 1 || field[0] !== 'name' || typeof error?.['message'] !== 'string') {
      throw new Error(`Expected ${context} name userError; got ${JSON.stringify(error)}.`);
    }
  }
}

function readFulfillmentServiceId(
  payload: ConformanceGraphqlPayload,
  root: 'setupUpdateSource' | 'fulfillmentServiceCreate',
): string {
  const fieldPayload = mutationPayload(payload, root);
  const service = readObject(fieldPayload['fulfillmentService']);
  const id = service?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`Expected ${root} to return a fulfillmentService id: ${JSON.stringify(payload)}`);
  }

  return id;
}

async function cleanup(id: string, deleteDocument: string): Promise<void> {
  try {
    const result = await client.runGraphqlRequest(deleteDocument, { id });
    if (result.status < 200 || result.status >= 300 || result.payload.errors) {
      console.error(`Failed to cleanup fulfillment service ${id}: ${JSON.stringify(result.payload)}`);
    }
  } catch (error) {
    console.error(`Failed to cleanup fulfillment service ${id}:`, error);
  }
}

const primaryDocument = await readRequest('fulfillment-service-name-whitespace-primary.graphql');
const updateDocument = await readRequest('fulfillment-service-name-whitespace-update.graphql');
const deleteDocument = await readRequest('fulfillment-service-lifecycle-delete.graphql');
const token = `fsws-${Date.now().toString(36)}`;
const cleanupIds: string[] = [];

const primaryVariables = {
  createName: `  FS Whitespace ${token}  `,
  setupName: `FS Whitespace Setup ${token}`,
};

let primary: ConformanceGraphqlResult | null = null;
let updatePrefix: ConformanceGraphqlResult | null = null;

try {
  primary = await client.runGraphqlRequest(primaryDocument, primaryVariables);
  assertNameUserErrors(primary, 'createWhitespace', 'create whitespace-prefix/postfix');
  assertNoUserErrors(primary, 'setupUpdateSource', 'setup update source');
  const serviceId = readFulfillmentServiceId(primary.payload, 'setupUpdateSource');
  cleanupIds.push(serviceId);

  const updatePrefixVariables = {
    id: serviceId,
    name: ` ${primaryVariables.setupName} Updated`,
  };
  updatePrefix = await client.runGraphqlRequest(updateDocument, updatePrefixVariables);
  assertNameUserErrors(updatePrefix, 'fulfillmentServiceUpdate', 'update whitespace-prefix');

  const fixture = {
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    token,
    notes: [
      'Live fulfillmentService name whitespace validation capture.',
      'The primary mutation records Shopify rejecting a create name with both leading and trailing whitespace, then creating one disposable fulfillment service used for update validation.',
      'The update mutation records Shopify rejecting a name with leading whitespace. The disposable setup service is deleted after capture.',
      'The active public schema exposes fulfillmentServiceCreate/Update.userErrors as UserError without a selectable code field; parity records field/message.',
    ],
    primary: {
      documentPath: 'config/parity-requests/shipping-fulfillments/fulfillment-service-name-whitespace-primary.graphql',
      variables: primaryVariables,
      response: primary,
    },
    updatePrefix: {
      documentPath: 'config/parity-requests/shipping-fulfillments/fulfillment-service-name-whitespace-update.graphql',
      variables: updatePrefixVariables,
      response: updatePrefix,
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
