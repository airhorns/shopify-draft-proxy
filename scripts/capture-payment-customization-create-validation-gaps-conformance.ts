/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
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
const outputPath = path.join(outputDir, 'payment-customization-create-validation-gaps.json');
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
  return typeof value === 'string' ? value : null;
}

function responseData(result: ConformanceGraphqlResult): JsonRecord {
  return readRecord(result.payload.data) ?? {};
}

function payloadId(result: ConformanceGraphqlResult, alias: string): string | null {
  const payload = readRecord(responseData(result)[alias]);
  const customization = readRecord(payload?.['paymentCustomization']);
  return readString(customization?.['id']);
}

function paymentCustomizationNodes(result: ConformanceGraphqlResult): JsonRecord[] {
  const data = readRecord(result.payload.data);
  const connection = readRecord(data?.['paymentCustomizations']);
  return readArray(connection?.['nodes'])
    .map(readRecord)
    .filter((node): node is JsonRecord => node !== null);
}

async function deletePaymentCustomization(id: string): Promise<ConformanceGraphqlResult> {
  return await runGraphqlRequest(deleteDocument, { id });
}

const functionCatalogDocument = `#graphql
  query PaymentCustomizationCreateValidationFunctionCatalog {
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

const activePaymentCustomizationsDocument = `#graphql
  query PaymentCustomizationCreateValidationActiveSet {
    paymentCustomizations(first: 50, query: "enabled:true") {
      nodes {
        id
        title
        enabled
        functionId
      }
    }
  }
`;

const validationDocument = `#graphql
  mutation PaymentCustomizationCreateValidationGaps(
    $missingMetafields: PaymentCustomizationInput!
    $bothIdentifiers: PaymentCustomizationInput!
    $missingIdentifier: PaymentCustomizationInput!
    $seed1: PaymentCustomizationInput!
    $seed2: PaymentCustomizationInput!
    $seed3: PaymentCustomizationInput!
    $seed4: PaymentCustomizationInput!
    $seed5: PaymentCustomizationInput!
    $overflow: PaymentCustomizationInput!
  ) {
    missingMetafields: paymentCustomizationCreate(paymentCustomization: $missingMetafields) {
      paymentCustomization { id }
      userErrors { field code message }
    }
    bothIdentifiers: paymentCustomizationCreate(paymentCustomization: $bothIdentifiers) {
      paymentCustomization { id }
      userErrors { field code message }
    }
    missingIdentifier: paymentCustomizationCreate(paymentCustomization: $missingIdentifier) {
      paymentCustomization { id }
      userErrors { field code message }
    }
    seed1: paymentCustomizationCreate(paymentCustomization: $seed1) {
      paymentCustomization { id title enabled functionId }
      userErrors { field code message }
    }
    seed2: paymentCustomizationCreate(paymentCustomization: $seed2) {
      paymentCustomization { id title enabled functionId }
      userErrors { field code message }
    }
    seed3: paymentCustomizationCreate(paymentCustomization: $seed3) {
      paymentCustomization { id title enabled functionId }
      userErrors { field code message }
    }
    seed4: paymentCustomizationCreate(paymentCustomization: $seed4) {
      paymentCustomization { id title enabled functionId }
      userErrors { field code message }
    }
    seed5: paymentCustomizationCreate(paymentCustomization: $seed5) {
      paymentCustomization { id title enabled functionId }
      userErrors { field code message }
    }
    overflow: paymentCustomizationCreate(paymentCustomization: $overflow) {
      paymentCustomization { id title enabled functionId }
      userErrors { field code message }
    }
  }
`;

const deleteDocument = `#graphql
  mutation PaymentCustomizationCreateValidationCleanup($id: ID!) {
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

if (!functionNode || typeof functionNode['id'] !== 'string' || typeof functionNode['handle'] !== 'string') {
  throw new Error(`No payment_customization ShopifyFunction is visible: ${JSON.stringify(functionCatalog.payload)}`);
}

const setupActive = await runGraphqlRequest(activePaymentCustomizationsDocument);
assertNoTopLevelErrors(setupActive, 'paymentCustomizations active setup query');
const setupDeletes: JsonRecord[] = [];
for (const node of paymentCustomizationNodes(setupActive)) {
  const id = readString(node['id']);
  if (!id) continue;
  const deleteResult = await deletePaymentCustomization(id);
  setupDeletes.push({ id, response: deleteResult.payload });
  assertNoTopLevelErrors(deleteResult, `paymentCustomizationDelete setup cleanup ${id}`);
}

const runId = Date.now();
const titlePrefix = `Draft proxy payment customization validation ${runId}`;
const functionId = functionNode['id'];
const functionHandle = functionNode['handle'];
const variables = {
  missingMetafields: {
    title: `${titlePrefix} missing metafields`,
    enabled: true,
    functionId,
  },
  bothIdentifiers: {
    title: `${titlePrefix} both identifiers`,
    enabled: true,
    functionId,
    functionHandle,
    metafields: [],
  },
  missingIdentifier: {
    title: `${titlePrefix} missing identifier`,
    enabled: true,
    metafields: [],
  },
  seed1: {
    title: `${titlePrefix} seed 1`,
    enabled: true,
    functionId,
    metafields: [],
  },
  seed2: {
    title: `${titlePrefix} seed 2`,
    enabled: true,
    functionId,
    metafields: [],
  },
  seed3: {
    title: `${titlePrefix} seed 3`,
    enabled: true,
    functionId,
    metafields: [],
  },
  seed4: {
    title: `${titlePrefix} seed 4`,
    enabled: true,
    functionId,
    metafields: [],
  },
  seed5: {
    title: `${titlePrefix} seed 5`,
    enabled: true,
    functionId,
    metafields: [],
  },
  overflow: {
    title: `${titlePrefix} overflow`,
    enabled: true,
    functionId,
    metafields: [],
  },
};

let validationResult: ConformanceGraphqlResult | null = null;
const cleanup: JsonRecord[] = [];

try {
  validationResult = await runGraphqlRequest(validationDocument, variables);
  assertNoTopLevelErrors(validationResult, 'paymentCustomizationCreate validation gaps');
} finally {
  if (validationResult) {
    for (const alias of ['missingMetafields', 'seed1', 'seed2', 'seed3', 'seed4', 'seed5', 'overflow']) {
      const id = payloadId(validationResult, alias);
      if (!id) continue;
      const deleteResult = await deletePaymentCustomization(id);
      cleanup.push({ id, response: deleteResult.payload });
    }
  }
}

if (!validationResult) {
  throw new Error('paymentCustomizationCreate validation gaps did not execute.');
}

const fixture = {
  scenarioId: 'payment-customization-create-validation-gaps',
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  functionCatalog: functionCatalog.payload,
  selectedFunction: functionNode,
  setup: {
    activeBeforeCapture: setupActive.payload,
    deletedActiveBeforeCapture: setupDeletes,
  },
  query: validationDocument,
  variables,
  response: {
    status: validationResult.status,
    payload: validationResult.payload,
  },
  cleanup,
  upstreamCalls: [],
  notes:
    'Captured against a disposable shop after deleting active PaymentCustomization rows. Public Shopify 2026-04 accepted the missing-metafields and sixth-active create branches in this test shop; Function identifier validation branches are executable parity targets.',
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
