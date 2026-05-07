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
const outputPath = path.join(outputDir, 'fulfillment-service-uniqueness.json');

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'shipping-fulfillments', name), 'utf8');
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readMutationPayload(
  payload: ConformanceGraphqlPayload,
  root: 'fulfillmentServiceCreate' | 'fulfillmentServiceUpdate' | 'fulfillmentServiceDelete',
): JsonRecord {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.[root]);
  if (!mutationPayload) {
    throw new Error(`Expected ${root} payload: ${JSON.stringify(payload)}`);
  }

  return mutationPayload;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(
  result: ConformanceGraphqlResult,
  root: 'fulfillmentServiceCreate' | 'fulfillmentServiceUpdate' | 'fulfillmentServiceDelete',
  context: string,
): void {
  assertNoTopLevelErrors(result, context);
  const payload = readMutationPayload(result.payload, root);
  const userErrors = payload['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }
  throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors)}`);
}

function assertDuplicateNameUserError(
  result: ConformanceGraphqlResult,
  root: 'fulfillmentServiceCreate' | 'fulfillmentServiceUpdate',
  context: string,
): void {
  assertNoTopLevelErrors(result, context);
  const mutationPayload = readMutationPayload(result.payload, root);
  if (mutationPayload['fulfillmentService'] !== null) {
    throw new Error(
      `Expected ${context} fulfillmentService to be null; got ${JSON.stringify(
        mutationPayload['fulfillmentService'],
      )}.`,
    );
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 1) {
    throw new Error(`Expected ${context} to return exactly one userError; got ${JSON.stringify(userErrors)}.`);
  }
  const error = readObject(userErrors[0]);
  const field = error?.['field'];
  if (
    !Array.isArray(field) ||
    field.length !== 1 ||
    field[0] !== 'name' ||
    error?.['message'] !== 'Name has already been taken'
  ) {
    throw new Error(`Expected ${context} duplicate-name userError; got ${JSON.stringify(error)}.`);
  }
}

function readFulfillmentServiceId(payload: ConformanceGraphqlPayload, context: string): string {
  const mutationPayload = readMutationPayload(payload, 'fulfillmentServiceCreate');
  const service = readObject(mutationPayload['fulfillmentService']);
  const id = service?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`Expected ${context} to return a fulfillmentService id: ${JSON.stringify(payload)}`);
  }

  return id;
}

function readOptionalFulfillmentServiceId(payload: ConformanceGraphqlPayload): string | null {
  const mutationPayload = readMutationPayload(payload, 'fulfillmentServiceCreate');
  const service = readObject(mutationPayload['fulfillmentService']);
  const id = service?.['id'];
  return typeof id === 'string' ? id : null;
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

const createDocument = await readRequest('fulfillment-service-uniqueness-create.graphql');
const updateDocument = await readRequest('fulfillment-service-uniqueness-update.graphql');
const deleteDocument = await readRequest('fulfillment-service-lifecycle-delete.graphql');
const token = `fsuniq-${Date.now().toString(36)}`;

const nameA = `FS Unique Acme ${token}`;
const spacedName = `FS Unique AB ${token}`;
const handleCollisionName = `fs-unique-ab-${token.toLowerCase()}`;
const updateSourceName = `FS Unique Source ${token}`;
const updateTargetName = `FS Unique Target ${token}`;
const cleanupIds: string[] = [];

const createAVariables = { name: nameA };
const sameNameDuplicateVariables = { name: nameA };
const caseVariantDuplicateVariables = { name: nameA.toUpperCase() };
const createSpacedVariables = { name: spacedName };
const handleCollisionDuplicateVariables = { name: handleCollisionName };
const createUpdateSourceVariables = { name: updateSourceName };
const createUpdateTargetVariables = { name: updateTargetName };

let createA: ConformanceGraphqlResult | null = null;
let sameNameDuplicateCreate: ConformanceGraphqlResult | null = null;
let caseVariantDuplicateCreate: ConformanceGraphqlResult | null = null;
let createSpaced: ConformanceGraphqlResult | null = null;
let handleCollisionDuplicateCreate: ConformanceGraphqlResult | null = null;
let createUpdateSource: ConformanceGraphqlResult | null = null;
let createUpdateTarget: ConformanceGraphqlResult | null = null;
let updateToExistingName: ConformanceGraphqlResult | null = null;

try {
  createA = await client.runGraphqlRequest(createDocument, createAVariables);
  assertNoUserErrors(createA, 'fulfillmentServiceCreate', 'create A');
  cleanupIds.push(readFulfillmentServiceId(createA.payload, 'create A'));

  sameNameDuplicateCreate = await client.runGraphqlRequest(createDocument, sameNameDuplicateVariables);
  assertDuplicateNameUserError(sameNameDuplicateCreate, 'fulfillmentServiceCreate', 'same-name duplicate create');
  const unexpectedSameNameId = readOptionalFulfillmentServiceId(sameNameDuplicateCreate.payload);
  if (unexpectedSameNameId) cleanupIds.push(unexpectedSameNameId);

  caseVariantDuplicateCreate = await client.runGraphqlRequest(createDocument, caseVariantDuplicateVariables);
  assertDuplicateNameUserError(caseVariantDuplicateCreate, 'fulfillmentServiceCreate', 'case-variant duplicate create');
  const unexpectedCaseVariantId = readOptionalFulfillmentServiceId(caseVariantDuplicateCreate.payload);
  if (unexpectedCaseVariantId) cleanupIds.push(unexpectedCaseVariantId);

  createSpaced = await client.runGraphqlRequest(createDocument, createSpacedVariables);
  assertNoUserErrors(createSpaced, 'fulfillmentServiceCreate', 'create spaced name');
  cleanupIds.push(readFulfillmentServiceId(createSpaced.payload, 'create spaced name'));

  handleCollisionDuplicateCreate = await client.runGraphqlRequest(createDocument, handleCollisionDuplicateVariables);
  assertDuplicateNameUserError(
    handleCollisionDuplicateCreate,
    'fulfillmentServiceCreate',
    'handle-collision duplicate create',
  );
  const unexpectedHandleCollisionId = readOptionalFulfillmentServiceId(handleCollisionDuplicateCreate.payload);
  if (unexpectedHandleCollisionId) cleanupIds.push(unexpectedHandleCollisionId);

  createUpdateSource = await client.runGraphqlRequest(createDocument, createUpdateSourceVariables);
  assertNoUserErrors(createUpdateSource, 'fulfillmentServiceCreate', 'create update source');
  cleanupIds.push(readFulfillmentServiceId(createUpdateSource.payload, 'create update source'));

  createUpdateTarget = await client.runGraphqlRequest(createDocument, createUpdateTargetVariables);
  assertNoUserErrors(createUpdateTarget, 'fulfillmentServiceCreate', 'create update target');
  const updateTargetId = readFulfillmentServiceId(createUpdateTarget.payload, 'create update target');
  cleanupIds.push(updateTargetId);

  const updateToExistingNameVariables = {
    id: updateTargetId,
    name: updateSourceName,
  };
  updateToExistingName = await client.runGraphqlRequest(updateDocument, updateToExistingNameVariables);
  assertDuplicateNameUserError(updateToExistingName, 'fulfillmentServiceUpdate', 'update-to-existing-name');

  const fixture = {
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    token,
    notes: [
      'Live fulfillmentService uniqueness capture.',
      'Created disposable fulfillment services, then recorded Shopify rejecting same-name duplicate create, case-variant duplicate create, generated-handle collision create, and update-to-existing-name.',
      'The active app schema exposes fulfillmentServiceCreate/Update.userErrors as UserError without a selectable code field; focused runtime tests cover the proxy projecting null when clients select code.',
      'Rejected duplicate create/update branches returned fulfillmentService: null and userErrors[{ field: ["name"], message: "Name has already been taken" }].',
    ],
    createA: {
      documentPath: 'config/parity-requests/shipping-fulfillments/fulfillment-service-uniqueness-create.graphql',
      variables: createAVariables,
      payload: createA.payload,
    },
    sameNameDuplicateCreate: {
      documentPath: 'config/parity-requests/shipping-fulfillments/fulfillment-service-uniqueness-create.graphql',
      variables: sameNameDuplicateVariables,
      payload: sameNameDuplicateCreate.payload,
    },
    caseVariantDuplicateCreate: {
      documentPath: 'config/parity-requests/shipping-fulfillments/fulfillment-service-uniqueness-create.graphql',
      variables: caseVariantDuplicateVariables,
      payload: caseVariantDuplicateCreate.payload,
    },
    createSpaced: {
      documentPath: 'config/parity-requests/shipping-fulfillments/fulfillment-service-uniqueness-create.graphql',
      variables: createSpacedVariables,
      payload: createSpaced.payload,
    },
    handleCollisionDuplicateCreate: {
      documentPath: 'config/parity-requests/shipping-fulfillments/fulfillment-service-uniqueness-create.graphql',
      variables: handleCollisionDuplicateVariables,
      payload: handleCollisionDuplicateCreate.payload,
    },
    createUpdateSource: {
      documentPath: 'config/parity-requests/shipping-fulfillments/fulfillment-service-uniqueness-create.graphql',
      variables: createUpdateSourceVariables,
      payload: createUpdateSource.payload,
    },
    createUpdateTarget: {
      documentPath: 'config/parity-requests/shipping-fulfillments/fulfillment-service-uniqueness-create.graphql',
      variables: createUpdateTargetVariables,
      payload: createUpdateTarget.payload,
    },
    updateToExistingName: {
      documentPath: 'config/parity-requests/shipping-fulfillments/fulfillment-service-uniqueness-update.graphql',
      variables: updateToExistingNameVariables,
      payload: updateToExistingName.payload,
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
