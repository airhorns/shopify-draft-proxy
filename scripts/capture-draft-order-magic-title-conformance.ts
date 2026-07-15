/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createConformanceCapture, readRecord, requireString, type JsonRecord } from './conformance-capture-lib.js';
import { captureDraftProxyShopPricingHydrate } from './support/shopify/runtime-hydration-capture.js';

const scenarioId = 'draftOrderCreate-magic-title-not-canned';
const cap = await createConformanceCapture();
const shopPricingHydrate = await captureDraftProxyShopPricingHydrate((query, variables) =>
  cap.runGraphqlRequest(query, variables),
);

const fixturePath = cap.fixturePath('orders', 'draft-order-create-magic-title-not-canned.json');
const specPath = path.join('config', 'parity-specs', 'orders', `${scenarioId}.json`);
const createDocumentPath = 'config/parity-requests/orders/draftOrderCreate-magic-title-create.graphql';
const readDocumentPath = 'config/parity-requests/orders/draftOrderCreate-magic-title-read.graphql';

const createDocument = await cap.readRequest('orders', 'draftOrderCreate-magic-title-create.graphql');
const readDocument = await cap.readRequest('orders', 'draftOrderCreate-magic-title-read.graphql');

const deleteDocument = `#graphql
  mutation DraftOrderCreateMagicTitleCleanup($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function draftOrderFromCreate(payload: JsonRecord): JsonRecord {
  const root = cap.mutationRoot(payload, 'draftOrderCreate', 'draftOrderCreate magic-title create');
  const draftOrder = readRecord(root['draftOrder']);
  if (!draftOrder) {
    throw new Error(`draftOrderCreate magic-title create missing draftOrder: ${JSON.stringify(payload, null, 2)}`);
  }
  return draftOrder;
}

function draftOrderFromRead(payload: JsonRecord): JsonRecord {
  const draftOrder = readRecord(readRecord(payload['data'])?.['draftOrder']);
  if (!draftOrder) {
    throw new Error(`draftOrder magic-title read missing draftOrder: ${JSON.stringify(payload, null, 2)}`);
  }
  return draftOrder;
}

function assertNotCannedDraft(draftOrder: JsonRecord, label: string): void {
  const lineItems = readRecord(draftOrder['lineItems']);
  const firstLine = readRecord((lineItems?.['nodes'] as unknown[] | undefined)?.[0]);
  const subtotal = readRecord(readRecord(draftOrder['subtotalPriceSet'])?.['shopMoney']);
  const total = readRecord(readRecord(draftOrder['totalPriceSet'])?.['shopMoney']);

  if (draftOrder['totalQuantityOfLineItems'] !== 2) {
    throw new Error(`${label} did not preserve submitted quantity: ${JSON.stringify(draftOrder, null, 2)}`);
  }
  if (subtotal?.['amount'] !== '6.5' || total?.['amount'] !== '6.5') {
    throw new Error(`${label} did not preserve submitted totals: ${JSON.stringify(draftOrder, null, 2)}`);
  }
  if (firstLine?.['sku'] !== 'MAGIC-TITLE-NOT-CANNED' || firstLine?.['requiresShipping'] !== false) {
    throw new Error(`${label} did not preserve submitted line item fields: ${JSON.stringify(draftOrder, null, 2)}`);
  }
}

function expectedDraftOrderDifferences(): JsonRecord[] {
  return [
    {
      path: '$.id',
      matcher: 'shopify-gid:DraftOrder',
      reason:
        'The proxy creates a stable synthetic draft order ID while the capture contains the live Shopify draft order ID.',
    },
    {
      path: '$.name',
      matcher: 'non-empty-string',
      reason:
        'Shopify allocates draft order names from live store state; local staging uses a deterministic synthetic sequence.',
    },
    {
      path: '$.invoiceUrl',
      matcher: 'non-empty-string',
      reason: 'Invoice URLs are generated per draft order by Shopify and locally synthesized by the proxy.',
    },
    {
      path: '$.lineItems.nodes[*].id',
      matcher: 'shopify-gid:DraftOrderLineItem',
      reason:
        'The proxy creates stable synthetic draft order line item IDs while the capture contains live Shopify IDs.',
    },
  ];
}

function paritySpec(): JsonRecord {
  return {
    scenarioId,
    operationNames: ['draftOrderCreate', 'draftOrder'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'selected-fields', 'downstream-read-parity', 'runtime-staging'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes/orders.rs'],
    proxyRequest: {
      documentPath: createDocumentPath,
      variablesCapturePath: '$.create.variables',
      apiVersion: cap.apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'magic-title-create-user-errors',
          capturePath: '$.create.response.data.draftOrderCreate.userErrors',
          proxyPath: '$.data.draftOrderCreate.userErrors',
        },
        {
          name: 'magic-title-create-draft-order-not-canned',
          capturePath: '$.create.response.data.draftOrderCreate.draftOrder',
          proxyPath: '$.data.draftOrderCreate.draftOrder',
          expectedDifferences: expectedDraftOrderDifferences(),
        },
        {
          name: 'magic-title-downstream-read-not-canned',
          capturePath: '$.downstreamRead.response.data.draftOrder',
          proxyPath: '$.data.draftOrder',
          proxyRequest: {
            documentPath: readDocumentPath,
            apiVersion: cap.apiVersion,
            variables: {
              id: {
                fromPrimaryProxyPath: '$.data.draftOrderCreate.draftOrder.id',
              },
            },
          },
          expectedDifferences: expectedDraftOrderDifferences(),
        },
      ],
    },
    notes:
      'Live Shopify evidence that `Invoice error parity item` is just ordinary draftOrderCreate input data. The captured request submits quantity 2, SKU MAGIC-TITLE-NOT-CANNED, no shipping requirement, tags, and custom attributes; strict parity would have failed against the removed scenario-keyed canned #D1 draft path.',
  };
}

const createVariables: JsonRecord = {
  input: {
    email: `draft-order-magic-title-${cap.stamp}@example.com`,
    tags: ['draft-proxy-capture', 'magic-title-not-canned', cap.stamp],
    customAttributes: [{ key: 'capture', value: 'magic-title-not-canned' }],
    lineItems: [
      {
        title: 'Invoice error parity item',
        quantity: 2,
        originalUnitPriceWithCurrency: { amount: '3.25', currencyCode: 'CAD' },
        sku: 'MAGIC-TITLE-NOT-CANNED',
        requiresShipping: false,
        taxable: false,
        customAttributes: [{ key: 'line', value: 'submitted-through-normal-create' }],
      },
    ],
  },
};

let draftOrderId: string | null = null;
let cleanup: JsonRecord | null = null;

try {
  const createResponse = await cap.run(createDocument, createVariables, 'draftOrderCreate magic-title create');
  const createdDraftOrder = draftOrderFromCreate(createResponse);
  assertNotCannedDraft(createdDraftOrder, 'create response');
  draftOrderId = requireString(createdDraftOrder['id'], 'created magic-title draft order id');

  const readVariables: JsonRecord = { id: draftOrderId };
  const downstreamReadResponse = await cap.run(readDocument, readVariables, 'draftOrder magic-title downstream read');
  assertNotCannedDraft(draftOrderFromRead(downstreamReadResponse), 'downstream read response');

  const cleanupResult = await cap.runGraphqlRequest(deleteDocument, { input: { id: draftOrderId } });
  cleanup = {
    variables: { input: { id: draftOrderId } },
    status: cleanupResult.status,
    response: cleanupResult.payload,
  };

  await cap.writeJson(fixturePath, {
    scenarioId,
    apiVersion: cap.apiVersion,
    storeDomain: cap.storeDomain,
    recordedAt: new Date().toISOString(),
    source: 'live-shopify-admin-graphql',
    safetyPolicy:
      'Creates one disposable draft order whose first custom line title is the formerly magic audit value, records Shopify create/read behavior, then deletes the draft order during cleanup.',
    create: {
      document: createDocument,
      variables: createVariables,
      response: createResponse,
    },
    downstreamRead: {
      document: readDocument,
      variables: readVariables,
      response: downstreamReadResponse,
    },
    cleanup: {
      draftOrderDelete: cleanup,
    },
    upstreamCalls: [shopPricingHydrate],
  });
  await writeJson(specPath, paritySpec());

  console.log(
    JSON.stringify(
      {
        ok: true,
        fixturePath,
        specPath,
        draftOrderId,
        cleanupStatus: cleanupResult.status,
      } satisfies JsonRecord,
      null,
      2,
    ),
  );
} finally {
  if (draftOrderId && cleanup === null) {
    const cleanupResult = await cap.runGraphqlRequest(deleteDocument, { input: { id: draftOrderId } });
    console.error(
      JSON.stringify(
        {
          cleanupAfterError: {
            draftOrderId,
            status: cleanupResult.status,
            response: cleanupResult.payload,
          },
        } satisfies JsonRecord,
        null,
        2,
      ),
    );
  }
}
