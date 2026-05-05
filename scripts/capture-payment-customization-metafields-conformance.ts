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
const outputPath = path.join(outputDir, 'payment-customization-metafields-and-handle-update.json');
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
    metafields(first: 5) {
      edges {
        node {
          id
          namespace
          key
          type
          value
          createdAt
          updatedAt
        }
      }
    }
  }
  userErrors {
    field
    code
    message
  }
`;

const functionCatalogDocument = `#graphql
  query PaymentCustomizationFunctionCatalog {
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
  mutation PaymentCustomizationMetafieldsCreate($input: PaymentCustomizationInput!) {
    paymentCustomizationCreate(paymentCustomization: $input) {
      ${paymentCustomizationSelection}
    }
  }
`;

const updateDocument = `#graphql
  mutation PaymentCustomizationMetafieldsUpdate($id: ID!, $input: PaymentCustomizationInput!) {
    paymentCustomizationUpdate(id: $id, paymentCustomization: $input) {
      ${paymentCustomizationSelection}
    }
  }
`;

const readDocument = `#graphql
  query PaymentCustomizationMetafieldsRead($id: ID!) {
    paymentCustomization(id: $id) {
      id
      title
      functionId
      metafields(first: 5) {
        edges {
          node {
            id
            namespace
            key
            type
            value
            createdAt
            updatedAt
          }
        }
      }
    }
  }
`;

const deleteDocument = `#graphql
  mutation PaymentCustomizationMetafieldsCleanup($id: ID!) {
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

const runId = Date.now();
const createVariables = {
  input: {
    title: `HAR-666 payment customization ${runId}`,
    enabled: true,
    functionId: functionNode['id'],
    metafields: [
      {
        namespace: '$app:har666',
        key: 'probe',
        type: 'single_line_text_field',
        value: 'baz',
      },
    ],
  },
};
const updateMetafieldsVariables = {
  id: '',
  input: {
    title: `HAR-666 payment customization updated ${runId}`,
    metafields: [
      {
        namespace: '$app:har666',
        key: 'probe',
        type: 'single_line_text_field',
        value: 'qux',
      },
    ],
  },
};
const updateHandleVariables = {
  id: '',
  input: {
    functionHandle: functionNode['handle'],
  },
};

let paymentCustomizationId: string | null = null;
const cleanup: JsonRecord = {};

try {
  const create = await runGraphqlRequest(createDocument, createVariables);
  assertNoTopLevelErrors(create, 'paymentCustomizationCreate with metafields');
  const createData = readRecord(create.payload.data);
  const createPayload = readRecord(createData?.['paymentCustomizationCreate']);
  const createdCustomization = readRecord(createPayload?.['paymentCustomization']);
  paymentCustomizationId = typeof createdCustomization?.['id'] === 'string' ? createdCustomization['id'] : null;
  if (!paymentCustomizationId) {
    throw new Error(`paymentCustomizationCreate did not return an id: ${JSON.stringify(create.payload, null, 2)}`);
  }
  const createdPaymentCustomizationId = paymentCustomizationId;

  updateMetafieldsVariables.id = paymentCustomizationId;
  const updateMetafields = await runGraphqlRequest(updateDocument, updateMetafieldsVariables);
  assertNoTopLevelErrors(updateMetafields, 'paymentCustomizationUpdate metafields');

  updateHandleVariables.id = paymentCustomizationId;
  const updateHandle = await runGraphqlRequest(updateDocument, updateHandleVariables);
  assertNoTopLevelErrors(updateHandle, 'paymentCustomizationUpdate functionHandle');

  const readAfterUpdates = await runGraphqlRequest(readDocument, { id: paymentCustomizationId });
  assertNoTopLevelErrors(readAfterUpdates, 'paymentCustomization read after updates');

  const deleteResult = await runGraphqlRequest(deleteDocument, { id: paymentCustomizationId });
  cleanup['paymentCustomizationDelete'] = deleteResult.payload;
  assertNoTopLevelErrors(deleteResult, 'paymentCustomizationDelete cleanup');
  paymentCustomizationId = null;

  const fixture = {
    scenarioId: 'payment-customization-metafields-and-handle-update',
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
      paymentCustomizationUpdateMetafields: {
        query: updateDocument,
        variables: updateMetafieldsVariables,
        response: updateMetafields.payload,
      },
      paymentCustomizationUpdateHandle: {
        query: updateDocument,
        variables: updateHandleVariables,
        response: updateHandle.payload,
      },
    },
    reads: {
      afterUpdates: {
        query: readDocument,
        variables: { id: createdPaymentCustomizationId },
        response: readAfterUpdates.payload,
      },
    },
    cleanup,
    upstreamCalls: [],
    notes:
      'Captured against a disposable PaymentCustomization. Shopify Admin 2026-04 accepts functionHandle as update input but does not expose paymentCustomization.functionHandle as a selected output field, so the capture proves the persisted Function reference through functionId.',
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
