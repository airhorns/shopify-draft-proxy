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
const outputPath = path.join(outputDir, 'payment-customization-update-immutable-function.json');
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

const paymentCustomizationSelection = `#graphql
  paymentCustomization {
    id
    title
    functionId
  }
  userErrors {
    field
    code
    message
  }
`;

const functionCatalogDocument = `#graphql
  query PaymentCustomizationImmutableFunctionCatalog {
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

const createDocument = `#graphql
  mutation PaymentCustomizationImmutableCreate($input: PaymentCustomizationInput!) {
    paymentCustomizationCreate(paymentCustomization: $input) {
      ${paymentCustomizationSelection}
    }
  }
`;

const updateDocument = `#graphql
  mutation PaymentCustomizationImmutableUpdate($id: ID!, $input: PaymentCustomizationInput!) {
    paymentCustomizationUpdate(id: $id, paymentCustomization: $input) {
      ${paymentCustomizationSelection}
    }
  }
`;

const readDocument = `#graphql
  query PaymentCustomizationImmutableRead($id: ID!) {
    paymentCustomization(id: $id) {
      id
      title
      functionId
    }
  }
`;

const deleteDocument = `#graphql
  mutation PaymentCustomizationImmutableCleanup($id: ID!) {
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

const replacementFunctionId = 'gid://shopify/ShopifyFunction/HAR629DifferentFunction';
const runId = Date.now();
const createVariables = {
  input: {
    title: `HAR-629 immutable function ${runId}`,
    enabled: true,
    functionId: functionNode['id'],
  },
};
const updateVariables = {
  id: '',
  input: {
    functionId: replacementFunctionId,
  },
};

let paymentCustomizationId: string | null = null;
const cleanup: JsonRecord = {};

try {
  const create = await runGraphqlRequest(createDocument, createVariables);
  assertNoTopLevelErrors(create, 'paymentCustomizationCreate immutable setup');
  const createData = readRecord(create.payload.data);
  const createPayload = readRecord(createData?.['paymentCustomizationCreate']);
  const createdCustomization = readRecord(createPayload?.['paymentCustomization']);
  paymentCustomizationId = typeof createdCustomization?.['id'] === 'string' ? createdCustomization['id'] : null;
  if (!paymentCustomizationId) {
    throw new Error(`paymentCustomizationCreate did not return an id: ${JSON.stringify(create.payload, null, 2)}`);
  }

  updateVariables.id = paymentCustomizationId;
  const immutableUpdate = await runGraphqlRequest(updateDocument, updateVariables);
  assertNoTopLevelErrors(immutableUpdate, 'paymentCustomizationUpdate immutable function');

  const readAfterImmutableUpdate = await runGraphqlRequest(readDocument, { id: paymentCustomizationId });
  assertNoTopLevelErrors(readAfterImmutableUpdate, 'paymentCustomization read after immutable function update');

  const deleteResult = await runGraphqlRequest(deleteDocument, { id: paymentCustomizationId });
  cleanup['paymentCustomizationDelete'] = deleteResult.payload;
  assertNoTopLevelErrors(deleteResult, 'paymentCustomizationDelete immutable cleanup');
  const createdPaymentCustomizationId = paymentCustomizationId;
  paymentCustomizationId = null;

  const fixture = {
    scenarioId: 'payment-customization-update-immutable-function',
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    functionCatalog: functionCatalog.payload,
    selectedFunction: functionNode,
    replacementFunctionId,
    operations: {
      paymentCustomizationCreate: {
        query: createDocument,
        variables: createVariables,
        response: create.payload,
      },
      paymentCustomizationUpdateImmutable: {
        query: updateDocument,
        variables: updateVariables,
        response: immutableUpdate.payload,
      },
    },
    reads: {
      afterImmutableUpdate: {
        query: readDocument,
        variables: { id: createdPaymentCustomizationId },
        response: readAfterImmutableUpdate.payload,
      },
    },
    cleanup,
    upstreamCalls: [],
    notes:
      'Captured against a disposable PaymentCustomization. Shopify rejects replacement functionId input with FUNCTION_ID_CANNOT_BE_CHANGED and leaves the stored functionId unchanged.',
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
