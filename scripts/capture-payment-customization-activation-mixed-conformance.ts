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
const outputPath = path.join(outputDir, 'payment-customization-activation-mixed.json');
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

async function readText(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

const createDocument = await readText('config/parity-requests/payments/payment-customization-immutable-create.graphql');
const activationDocument = await readText(
  'config/parity-requests/payments/payment-customization-activation-mixed.graphql',
);

const functionCatalogDocument = `#graphql
  query PaymentCustomizationActivationFunctionCatalog {
    shopifyFunctions(first: 20) {
      nodes {
        id
        title
        handle
        apiType
      }
    }
  }
`;

const deleteDocument = `#graphql
  mutation PaymentCustomizationActivationCleanup($id: ID!) {
    paymentCustomizationDelete(id: $id) {
      deletedId
      userErrors {
        field
        code
        message
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
  throw new Error(`No payment_customization ShopifyFunction is visible: ${JSON.stringify(functionCatalog.payload)}`);
}

const runId = Date.now();
const createVariables = {
  input: {
    title: `Draft proxy payment customization activation ${runId}`,
    enabled: true,
    functionId: functionNode['id'],
    metafields: [],
  },
};
const missingId = 'gid://shopify/PaymentCustomization/0';
let paymentCustomizationId: string | null = null;
const cleanup: JsonRecord = {};

try {
  const create = await runGraphqlRequest(createDocument, createVariables);
  assertNoTopLevelErrors(create, 'paymentCustomizationCreate activation setup');
  const createPayload = readRecord(readRecord(create.payload.data)?.['paymentCustomizationCreate']);
  const createdCustomization = readRecord(createPayload?.['paymentCustomization']);
  paymentCustomizationId = readString(createdCustomization?.['id']);
  if (!paymentCustomizationId) {
    throw new Error(`paymentCustomizationCreate did not return an id: ${JSON.stringify(create.payload, null, 2)}`);
  }

  const activationVariables = {
    ids: [paymentCustomizationId, missingId],
    enabled: false,
  };
  const activation = await runGraphqlRequest(activationDocument, activationVariables);
  assertNoTopLevelErrors(activation, 'paymentCustomizationActivation mixed ids');

  const deleteResult = await runGraphqlRequest(deleteDocument, { id: paymentCustomizationId });
  cleanup['paymentCustomizationDelete'] = deleteResult.payload;
  assertNoTopLevelErrors(deleteResult, 'paymentCustomizationDelete cleanup');
  paymentCustomizationId = null;

  const fixture = {
    scenarioId: 'payment-customization-activation-mixed',
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    functionCatalog: functionCatalog.payload,
    selectedFunction: functionNode,
    operations: {
      paymentCustomizationCreate: {
        query: createDocument,
        variables: createVariables,
        response: create.payload,
      },
      paymentCustomizationActivationMixed: {
        query: activationDocument,
        variables: activationVariables,
        response: activation.payload,
      },
    },
    cleanup,
    upstreamCalls: [],
    notes:
      'Captured against a disposable PaymentCustomization. The activation request submits the valid created id plus a known-missing id, proving Shopify returns only the valid mutated id in ids and reports the missing id through userErrors.',
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (paymentCustomizationId) {
    cleanup['paymentCustomizationDelete'] = (
      await runGraphqlRequest(deleteDocument, { id: paymentCustomizationId })
    ).payload;
  }
}
