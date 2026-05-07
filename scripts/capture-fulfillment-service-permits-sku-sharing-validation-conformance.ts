/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CapturedRequest = {
  documentPath: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
};

const scenarioId = 'fulfillment-service-permits-sku-sharing-validation';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

const removedArgumentDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-permits-sku-sharing-validation.graphql',
);
const createDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-inventory-management-create.graphql',
);
const readDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-inventory-management-read.graphql',
);
const updateDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-inventory-management-update.graphql',
);

const deleteDocument = `#graphql
  mutation FulfillmentServicePermitsSkuSharingCleanup($id: ID!) {
    fulfillmentServiceDelete(id: $id, inventoryAction: DELETE) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const schemaProbeDocument = `#graphql
  query FulfillmentServicePermitsSkuSharingSchemaProbe {
    fulfillmentServiceType: __type(name: "FulfillmentService") {
      fields {
        name
      }
    }
    schema: __schema {
      mutationType {
        fields {
          name
          args {
            name
            defaultValue
          }
        }
      }
    }
  }
`;

async function readText(relativePath: string): Promise<string> {
  return readFile(path.join(process.cwd(), relativePath), 'utf8');
}

async function capture(documentPath: string, variables: JsonRecord): Promise<CapturedRequest> {
  const document = await readText(documentPath);
  return {
    documentPath,
    variables,
    response: await runGraphqlRequest(document, variables),
  };
}

async function captureAdHoc(
  query: string,
  variables: JsonRecord,
): Promise<{
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
}> {
  const trimmed = query.replace(/^#graphql\n/u, '').trim();
  return {
    query: trimmed,
    variables,
    response: await runGraphqlRequest(trimmed, variables),
  };
}

function isObject(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function payloadData(captureResult: CapturedRequest): JsonRecord {
  const data = captureResult.response.payload.data;
  if (!isObject(data)) {
    throw new Error(`Expected payload data: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return data;
}

function mutationPayload(captureResult: CapturedRequest, root: string): JsonRecord {
  const rootPayload = payloadData(captureResult)[root];
  if (!isObject(rootPayload)) {
    throw new Error(`Expected ${root} payload: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return rootPayload;
}

function readFulfillmentService(captureResult: CapturedRequest, root: string): JsonRecord {
  const payload = mutationPayload(captureResult, root);
  const service = payload['fulfillmentService'];
  if (!isObject(service)) {
    throw new Error(`Expected ${root}.fulfillmentService object: ${JSON.stringify(payload)}`);
  }
  return service;
}

function userErrors(captureResult: CapturedRequest, root: string): JsonRecord[] {
  const errors = mutationPayload(captureResult, root)['userErrors'];
  return Array.isArray(errors) ? errors.filter(isObject) : [];
}

function assertNoUserErrors(captureResult: CapturedRequest, root: string): void {
  const errors = userErrors(captureResult, root);
  if (captureResult.response.status !== 200 || captureResult.response.payload.errors || errors.length !== 0) {
    throw new Error(`${root} expected no userErrors, got ${JSON.stringify(captureResult.response)}`);
  }
}

function assertArgumentNotAccepted(result: ConformanceGraphqlResult, argumentName: string): void {
  const errors = Array.isArray(result.payload.errors) ? result.payload.errors : [];
  const match = errors.find((error): error is JsonRecord => {
    if (!isObject(error) || !isObject(error['extensions'])) return false;
    return (
      error['extensions']['code'] === 'argumentNotAccepted' && error['extensions']['argumentName'] === argumentName
    );
  });
  if (result.status !== 200 || !match) {
    throw new Error(`Expected argumentNotAccepted for ${argumentName}, got ${JSON.stringify(result.payload)}`);
  }
}

function readArgs(schemaProbe: ConformanceGraphqlResult, root: string): string[] {
  const data = schemaProbe.payload.data;
  if (!isObject(data)) return [];
  const schema = data['schema'];
  if (!isObject(schema)) return [];
  const mutationType = schema['mutationType'];
  if (!isObject(mutationType)) return [];
  const fields = mutationType['fields'];
  if (!Array.isArray(fields)) return [];
  const field = fields.filter(isObject).find((entry) => entry['name'] === root);
  const args = isObject(field) ? field['args'] : undefined;
  return Array.isArray(args)
    ? args
        .filter(isObject)
        .map((arg) => arg['name'])
        .filter((name): name is string => typeof name === 'string')
    : [];
}

function readFulfillmentServiceFields(schemaProbe: ConformanceGraphqlResult): string[] {
  const data = schemaProbe.payload.data;
  if (!isObject(data)) return [];
  const type = data['fulfillmentServiceType'];
  if (!isObject(type)) return [];
  const fields = type['fields'];
  return Array.isArray(fields)
    ? fields
        .filter(isObject)
        .map((field) => field['name'])
        .filter((name): name is string => typeof name === 'string')
    : [];
}

const suffix = Date.now().toString(36);
const removedArgsVariables = { name: `FS Removed Args ${suffix}` };
const createVariables = { name: `FS Inventory Management ${suffix}` };
const updateVariables = { id: '', name: `FS Inventory Management Updated ${suffix}` };

const schemaProbe = await runGraphqlRaw(schemaProbeDocument, {});
const createArgs = readArgs(schemaProbe, 'fulfillmentServiceCreate');
const updateArgs = readArgs(schemaProbe, 'fulfillmentServiceUpdate');
const fulfillmentServiceFields = readFulfillmentServiceFields(schemaProbe);

for (const removedName of ['permitsSkuSharing', 'inventorySyncEnabled', 'fulfillmentOrdersOptIn']) {
  if (createArgs.includes(removedName) || updateArgs.includes(removedName)) {
    throw new Error(
      `Expected public Admin ${apiVersion} schema not to expose ${removedName}; create=${JSON.stringify(
        createArgs,
      )} update=${JSON.stringify(updateArgs)}`,
    );
  }
}

const removedArgumentValidation = await capture(removedArgumentDocumentPath, removedArgsVariables);
assertArgumentNotAccepted(removedArgumentValidation.response, 'permitsSkuSharing');

const cleanup: Awaited<ReturnType<typeof captureAdHoc>>[] = [];
let createSuccess: CapturedRequest | null = null;
let readAfterCreate: CapturedRequest | null = null;
let updateSuccess: CapturedRequest | null = null;
let readAfterUpdate: CapturedRequest | null = null;

try {
  createSuccess = await capture(createDocumentPath, createVariables);
  assertNoUserErrors(createSuccess, 'fulfillmentServiceCreate');
  const created = readFulfillmentService(createSuccess, 'fulfillmentServiceCreate');
  const serviceId = created['id'];
  if (typeof serviceId !== 'string') {
    throw new Error(`Expected create success to return id: ${JSON.stringify(created)}`);
  }
  if (created['inventoryManagement'] !== true) {
    throw new Error(`Expected create success inventoryManagement true: ${JSON.stringify(created)}`);
  }

  readAfterCreate = await capture(readDocumentPath, { id: serviceId });
  const readCreateService = payloadData(readAfterCreate)['fulfillmentService'];
  if (!isObject(readCreateService) || readCreateService['inventoryManagement'] !== true) {
    throw new Error(
      `Expected downstream read after create to show inventoryManagement true: ${JSON.stringify(readCreateService)}`,
    );
  }

  updateVariables.id = serviceId;
  updateSuccess = await capture(updateDocumentPath, updateVariables);
  assertNoUserErrors(updateSuccess, 'fulfillmentServiceUpdate');
  const updated = readFulfillmentService(updateSuccess, 'fulfillmentServiceUpdate');
  if (updated['inventoryManagement'] !== false) {
    throw new Error(`Expected update success inventoryManagement false: ${JSON.stringify(updated)}`);
  }

  readAfterUpdate = await capture(readDocumentPath, { id: serviceId });
  const readUpdateService = payloadData(readAfterUpdate)['fulfillmentService'];
  if (!isObject(readUpdateService) || readUpdateService['inventoryManagement'] !== false) {
    throw new Error(
      `Expected downstream read after update to show inventoryManagement false: ${JSON.stringify(readUpdateService)}`,
    );
  }
} finally {
  if (updateVariables.id) {
    cleanup.push(await captureAdHoc(deleteDocument, { id: updateVariables.id }));
  }
}

if (!createSuccess || !readAfterCreate || !updateSuccess || !readAfterUpdate) {
  throw new Error('Expected all success/read captures to be present.');
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId,
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      schemaProbe: {
        query: schemaProbeDocument.replace(/^#graphql\n/u, '').trim(),
        response: schemaProbe,
        fulfillmentServiceFields,
        createArgs,
        updateArgs,
      },
      notes: [
        `Captured against ${storeDomain} using Admin GraphQL ${apiVersion}.`,
        'The current public schema does not expose fulfillmentServiceCreate/Update permitsSkuSharing, inventorySyncEnabled, or fulfillmentOrdersOptIn arguments; permitsSkuSharing: false fails GraphQL validation before resolver execution.',
        'FulfillmentService downstream reads expose inventoryManagement in this public schema; permitsSkuSharing, inventorySyncEnabled, and fulfillmentOrdersOptIn are not selectable fields for this credential.',
        'The successful create/update branches prove inventoryManagement read-after-write visibility and are cleaned up with fulfillmentServiceDelete.',
      ],
      removedArgumentValidation,
      createSuccess,
      readAfterCreate,
      updateSuccess,
      readAfterUpdate,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath: outputPath }, null, 2));
