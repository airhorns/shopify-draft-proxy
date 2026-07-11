/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'payments');
const outputPath = path.join(outputDir, 'payment-customization-mutation-first-hydration.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type JsonRecord = Record<string, unknown>;

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function rootPayload(result: ConformanceGraphqlResult, root: string): JsonRecord | null {
  return readRecord(readRecord(result.payload.data)?.[root]);
}

function assertNoUserErrors(result: ConformanceGraphqlResult, root: string, context: string): void {
  const payload = rootPayload(result, root);
  const userErrors = readArray(payload?.['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function payloadCustomizationId(result: ConformanceGraphqlResult, root: string): string {
  const id = readString(readRecord(rootPayload(result, root)?.['paymentCustomization'])?.['id']);
  if (!id) {
    throw new Error(`${root} did not return a paymentCustomization id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return id;
}

async function readText(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

const createDocument = await readText('config/parity-requests/payments/payment-customization-immutable-create.graphql');
const updateDocument = await readText(
  'config/parity-requests/payments/payment-customization-mutation-first-update.graphql',
);
const activationDocument = await readText(
  'config/parity-requests/payments/payment-customization-activation-mixed.graphql',
);
const deleteDocument = await readText(
  'config/parity-requests/payments/payment-customization-mutation-first-delete.graphql',
);
const readDocument = await readText(
  'config/parity-requests/payments/payment-customization-mutation-first-read.graphql',
);
const hydrateByIdDocument = await readText(
  'config/parity-requests/payments/payment-customization-hydrate-by-id.graphql',
);
const hydrateCatalogDocument = await readText(
  'config/parity-requests/payments/payment-customization-hydrate-catalog.graphql',
);

const functionCatalogDocument = `#graphql
  query PaymentCustomizationMutationFirstFunctionCatalog {
    shopifyFunctions(first: 50) {
      nodes {
        id
        title
        handle
        apiType
        description
        appKey
        app {
          id
          title
          handle
          apiKey
        }
      }
    }
  }
`;

await mkdir(outputDir, { recursive: true });

const functionCatalog = await runGraphqlRequest(functionCatalogDocument);
assertNoTopLevelErrors(functionCatalog, 'shopifyFunctions payment customization catalog');
const functionCatalogData = readRecord(functionCatalog.payload.data);
const shopifyFunctions = readRecord(functionCatalogData?.['shopifyFunctions']);
const functionNode =
  readArray(shopifyFunctions?.['nodes'])
    .map(readRecord)
    .find((node): node is JsonRecord => node?.['apiType'] === 'payment_customization') ?? null;

if (!functionNode || typeof functionNode['id'] !== 'string') {
  throw new Error(
    [
      'No payment_customization ShopifyFunction is visible to this app.',
      'This capture requires a Shopify Plus-capable test shop with read_payment_customizations/write_payment_customizations scopes and a released payment customization function.',
      JSON.stringify(functionCatalog.payload),
    ].join(' '),
  );
}

const runId = Date.now();
const titlePrefix = `Draft proxy payment customization mutation first ${runId}`;
const functionId = functionNode['id'];
const metafields = {
  namespace: '$app:mutation_first',
  key: 'probe',
  type: 'single_line_text_field',
};
const cleanupIds = new Set<string>();
const cleanup: JsonRecord[] = [];

async function createPaymentCustomization(
  label: string,
  enabled: boolean,
  value: string,
): Promise<{
  id: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
}> {
  const variables = {
    input: {
      title: `${titlePrefix} ${label}`,
      enabled,
      functionId,
      metafields: [{ ...metafields, value }],
    },
  };
  const response = await runGraphqlRequest(createDocument, variables);
  assertNoTopLevelErrors(response, `paymentCustomizationCreate ${label}`);
  assertNoUserErrors(response, 'paymentCustomizationCreate', `paymentCustomizationCreate ${label}`);
  const id = payloadCustomizationId(response, 'paymentCustomizationCreate');
  cleanupIds.add(id);
  return { id, variables, response };
}

async function readPaymentCustomization(id: string): Promise<ConformanceGraphqlResult> {
  const response = await runGraphqlRequest(readDocument, { id });
  assertNoTopLevelErrors(response, `paymentCustomization read ${id}`);
  return response;
}

async function hydratePaymentCustomization(id: string, context: string): Promise<ConformanceGraphqlResult> {
  const response = await runGraphqlRequest(hydrateByIdDocument, { id });
  assertNoTopLevelErrors(response, context);
  if (!readRecord(readRecord(response.payload.data)?.['paymentCustomization'])) {
    throw new Error(`${context} returned null for ${id}: ${JSON.stringify(response.payload, null, 2)}`);
  }
  return response;
}

async function deletePaymentCustomization(id: string, context: string): Promise<ConformanceGraphqlResult> {
  const response = await runGraphqlRequest(deleteDocument, { id });
  assertNoTopLevelErrors(response, context);
  return response;
}

async function cleanupPaymentCustomization(id: string): Promise<void> {
  try {
    const response = await deletePaymentCustomization(id, `paymentCustomizationDelete cleanup ${id}`);
    cleanup.push({ id, response: response.payload });
  } catch (error) {
    cleanup.push({ id, error: (error as Error).message });
  } finally {
    cleanupIds.delete(id);
  }
}

let setupUpdate: Awaited<ReturnType<typeof createPaymentCustomization>> | null = null;
let setupActivation: Awaited<ReturnType<typeof createPaymentCustomization>> | null = null;
let setupDelete: Awaited<ReturnType<typeof createPaymentCustomization>> | null = null;
let updateHydrate: ConformanceGraphqlResult | null = null;
let activationHydrateCatalog: ConformanceGraphqlResult | null = null;
let activationHydrateById: ConformanceGraphqlResult | null = null;
let deleteHydrate: ConformanceGraphqlResult | null = null;
let updateResult: ConformanceGraphqlResult | null = null;
let activationResult: ConformanceGraphqlResult | null = null;
let deleteResult: ConformanceGraphqlResult | null = null;
let afterUpdate: ConformanceGraphqlResult | null = null;
let afterActivation: ConformanceGraphqlResult | null = null;
let afterDelete: ConformanceGraphqlResult | null = null;

try {
  setupUpdate = await createPaymentCustomization('update base', true, 'base-update');
  setupActivation = await createPaymentCustomization('activation base', false, 'base-activation');
  setupDelete = await createPaymentCustomization('delete base', false, 'base-delete');

  updateHydrate = await hydratePaymentCustomization(setupUpdate.id, 'paymentCustomization hydrate before update');
  const updateVariables = {
    id: setupUpdate.id,
    input: {
      title: `${titlePrefix} updated`,
      enabled: false,
      functionId,
      metafields: [{ ...metafields, value: 'updated' }],
    },
  };
  updateResult = await runGraphqlRequest(updateDocument, updateVariables);
  assertNoTopLevelErrors(updateResult, 'paymentCustomizationUpdate mutation-first');
  assertNoUserErrors(updateResult, 'paymentCustomizationUpdate', 'paymentCustomizationUpdate mutation-first');
  afterUpdate = await readPaymentCustomization(setupUpdate.id);

  activationHydrateCatalog = await runGraphqlRequest(hydrateCatalogDocument, {});
  assertNoTopLevelErrors(activationHydrateCatalog, 'paymentCustomizations catalog hydrate before activation');
  activationHydrateById = await hydratePaymentCustomization(
    setupActivation.id,
    'paymentCustomization hydrate by id before activation fallback',
  );
  const activationVariables = {
    ids: [setupActivation.id],
    enabled: true,
  };
  activationResult = await runGraphqlRequest(activationDocument, activationVariables);
  assertNoTopLevelErrors(activationResult, 'paymentCustomizationActivation mutation-first');
  assertNoUserErrors(
    activationResult,
    'paymentCustomizationActivation',
    'paymentCustomizationActivation mutation-first',
  );
  afterActivation = await readPaymentCustomization(setupActivation.id);

  deleteHydrate = await hydratePaymentCustomization(setupDelete.id, 'paymentCustomization hydrate before delete');
  const deleteVariables = { id: setupDelete.id };
  deleteResult = await deletePaymentCustomization(setupDelete.id, 'paymentCustomizationDelete mutation-first');
  assertNoUserErrors(deleteResult, 'paymentCustomizationDelete', 'paymentCustomizationDelete mutation-first');
  cleanupIds.delete(setupDelete.id);
  afterDelete = await readPaymentCustomization(setupDelete.id);

  for (const id of cleanupIds) {
    await cleanupPaymentCustomization(id);
  }

  const fixture = {
    scenarioId: 'payment-customization-mutation-first-hydration',
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    functionCatalog: functionCatalog.payload,
    selectedFunction: functionNode,
    setup: {
      paymentCustomizationCreateUpdateTarget: {
        query: createDocument,
        variables: setupUpdate.variables,
        response: setupUpdate.response.payload,
      },
      paymentCustomizationCreateActivationTarget: {
        query: createDocument,
        variables: setupActivation.variables,
        response: setupActivation.response.payload,
      },
      paymentCustomizationCreateDeleteTarget: {
        query: createDocument,
        variables: setupDelete.variables,
        response: setupDelete.response.payload,
      },
    },
    operations: {
      paymentCustomizationUpdateMutationFirst: {
        query: updateDocument,
        variables: updateVariables,
        response: updateResult.payload,
      },
      paymentCustomizationActivationMutationFirst: {
        query: activationDocument,
        variables: activationVariables,
        response: activationResult.payload,
      },
      paymentCustomizationDeleteMutationFirst: {
        query: deleteDocument,
        variables: deleteVariables,
        response: deleteResult.payload,
      },
    },
    reads: {
      beforeUpdateHydrate: {
        query: hydrateByIdDocument,
        variables: { id: setupUpdate.id },
        response: updateHydrate.payload,
      },
      beforeActivationCatalogHydrate: {
        query: hydrateCatalogDocument,
        variables: {},
        response: activationHydrateCatalog.payload,
      },
      beforeActivationByIdHydrate: {
        query: hydrateByIdDocument,
        variables: { id: setupActivation.id },
        response: activationHydrateById.payload,
      },
      beforeDeleteHydrate: {
        query: hydrateByIdDocument,
        variables: { id: setupDelete.id },
        response: deleteHydrate.payload,
      },
      afterUpdate: {
        query: readDocument,
        variables: { id: setupUpdate.id },
        response: afterUpdate.payload,
      },
      afterActivation: {
        query: readDocument,
        variables: { id: setupActivation.id },
        response: afterActivation.payload,
      },
      afterDelete: {
        query: readDocument,
        variables: { id: setupDelete.id },
        response: afterDelete.payload,
      },
    },
    cleanup,
    upstreamCalls: [
      {
        operationName: 'PaymentCustomizationHydrateById',
        variables: { id: setupUpdate.id },
        query: hydrateByIdDocument,
        response: {
          status: updateHydrate.status,
          body: updateHydrate.payload,
        },
      },
      {
        operationName: 'PaymentCustomizationHydrateCatalog',
        variables: {},
        query: hydrateCatalogDocument,
        response: {
          status: activationHydrateCatalog.status,
          body: activationHydrateCatalog.payload,
        },
      },
      {
        operationName: 'PaymentCustomizationHydrateById',
        variables: { id: setupActivation.id },
        query: hydrateByIdDocument,
        response: {
          status: activationHydrateById.status,
          body: activationHydrateById.payload,
        },
      },
      {
        operationName: 'PaymentCustomizationHydrateById',
        variables: { id: setupDelete.id },
        query: hydrateByIdDocument,
        response: {
          status: deleteHydrate.status,
          body: deleteHydrate.payload,
        },
      },
    ],
    notes:
      'Captured against disposable PaymentCustomization rows and a visible payment_customization ShopifyFunction. The proxy replay uses recorded upstream reads only: by-id hydrate before mutation-first update/delete, catalog hydrate before enabling activation, and by-id activation fallback if the catalog window misses the target. Requires read_payment_customizations/write_payment_customizations scopes and a Shopify Plus-capable test shop/app with a released payment customization function.',
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  for (const id of cleanupIds) {
    await cleanupPaymentCustomization(id);
  }
}
