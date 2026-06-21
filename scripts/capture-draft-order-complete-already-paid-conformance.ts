/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'draft-order-complete-already-paid.json');
const createRequestPath = path.join(
  'config',
  'parity-requests',
  'orders',
  'draftOrderComplete-already-paid-create.graphql',
);
const completeRequestPath = path.join(
  'config',
  'parity-requests',
  'orders',
  'draftOrderComplete-already-paid-complete.graphql',
);
const orderReadRequestPath = path.join(
  'config',
  'parity-requests',
  'orders',
  'draftOrderComplete-already-paid-order-read.graphql',
);

const draftOrderReadDocument = `#graphql
  query DraftOrderCompleteAlreadyPaidDraftRead($id: ID!) {
    draftOrder(id: $id) {
      id
      name
      status
      ready
      completedAt
      order {
        id
        name
      }
    }
  }
`;

const orderCancelDocument = `#graphql
  mutation DraftOrderCompleteAlreadyPaidCleanup($orderId: ID!, $reason: OrderCancelReason!, $notifyCustomer: Boolean!, $restock: Boolean!) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job {
        id
        done
      }
      userErrors {
        field
        message
      }
    }
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function readRequest(filePath: string): Promise<string> {
  return (await readFile(filePath, 'utf8')).trim();
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readRecord(value: unknown, key: string): JsonRecord | null {
  return asRecord(asRecord(value)?.[key]);
}

function readArray(value: unknown, key: string): unknown[] {
  const array = asRecord(value)?.[key];
  return Array.isArray(array) ? array : [];
}

function readString(value: unknown, key: string): string | null {
  const field = asRecord(value)?.[key];
  return typeof field === 'string' && field.length > 0 ? field : null;
}

function payloadData(result: ConformanceGraphqlResult<JsonRecord>): JsonRecord {
  const data = asRecord(result.payload.data);
  if (!data) throw new Error(`Missing GraphQL data payload: ${JSON.stringify(result.payload, null, 2)}`);
  return data;
}

function dataRoot(result: ConformanceGraphqlResult<JsonRecord>, root: string): JsonRecord {
  const value = payloadData(result)[root];
  const record = asRecord(value);
  if (!record) throw new Error(`Missing ${root} payload: ${JSON.stringify(result.payload, null, 2)}`);
  return record;
}

function assertHttpOk(label: string, result: ConformanceGraphqlResult<JsonRecord>): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertNoUserErrors(label: string, result: ConformanceGraphqlResult<JsonRecord>, root: string): void {
  const userErrors = readArray(dataRoot(result, root), 'userErrors');
  if (userErrors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function draftOrderIdFromCreate(result: ConformanceGraphqlResult<JsonRecord>): string {
  const draftOrder = readRecord(dataRoot(result, 'draftOrderCreate'), 'draftOrder');
  const id = readString(draftOrder, 'id');
  if (!id) throw new Error(`draftOrderCreate did not return a draft id: ${JSON.stringify(result.payload, null, 2)}`);
  return id;
}

function orderDetailsFromComplete(result: ConformanceGraphqlResult<JsonRecord>): { id: string; name: string } {
  const draftOrder = readRecord(dataRoot(result, 'draftOrderComplete'), 'draftOrder');
  const order = readRecord(draftOrder, 'order');
  const id = readString(order, 'id');
  const name = readString(order, 'name');
  if (!id) throw new Error(`draftOrderComplete did not return an order id: ${JSON.stringify(result.payload, null, 2)}`);
  if (!name) {
    throw new Error(`draftOrderComplete did not return an order name: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return { id, name };
}

function assertPaidError(result: ConformanceGraphqlResult<JsonRecord>): void {
  const userErrors = readArray(dataRoot(result, 'draftOrderComplete'), 'userErrors');
  if (
    !userErrors.some((error) => {
      const record = asRecord(error);
      return record?.['message'] === 'This order has been paid';
    })
  ) {
    throw new Error(
      `Second draftOrderComplete did not return the paid error: ${JSON.stringify(result.payload, null, 2)}`,
    );
  }
}

function assertOrderReadUnchanged(
  before: ConformanceGraphqlResult<JsonRecord>,
  after: ConformanceGraphqlResult<JsonRecord>,
): void {
  const beforeData = payloadData(before);
  const afterData = payloadData(after);
  if (JSON.stringify(beforeData) !== JSON.stringify(afterData)) {
    throw new Error(
      `Order read changed after redundant completion: ${JSON.stringify({ before: beforeData, after: afterData }, null, 2)}`,
    );
  }
}

function assertOrderReadMatches(orderRead: ConformanceGraphqlResult<JsonRecord>, orderId: string): void {
  const order = readRecord(payloadData(orderRead), 'order');
  if (readString(order, 'id') !== orderId) {
    throw new Error(
      `order(id:) did not return completed order ${orderId}: ${JSON.stringify(orderRead.payload, null, 2)}`,
    );
  }
}

async function main(): Promise<void> {
  const createDocument = await readRequest(createRequestPath);
  const completeDocument = await readRequest(completeRequestPath);
  const orderReadDocument = await readRequest(orderReadRequestPath);
  const stamp = Date.now();
  const shortStamp = stamp.toString(36);
  const tag = `dcap-${shortStamp}`;
  const createVariables = {
    input: {
      email: `draft-complete-already-paid-${stamp}@example.com`,
      note: 'draft order complete already paid conformance',
      tags: [tag, 'dcap'],
      lineItems: [
        {
          title: 'Already paid completion guard',
          quantity: 1,
          originalUnitPrice: '1.00',
          sku: `paid-guard-${stamp}`,
        },
      ],
    },
  };

  const createResult = await runGraphqlRequest<JsonRecord>(createDocument, createVariables);
  assertHttpOk('draftOrderCreate setup', createResult);
  assertNoUserErrors('draftOrderCreate setup', createResult, 'draftOrderCreate');
  const draftOrderId = draftOrderIdFromCreate(createResult);
  const completeVariables = { id: draftOrderId, paymentPending: false };

  const firstCompleteResult = await runGraphqlRequest<JsonRecord>(completeDocument, completeVariables);
  assertHttpOk('first draftOrderComplete', firstCompleteResult);
  assertNoUserErrors('first draftOrderComplete', firstCompleteResult, 'draftOrderComplete');
  const completedOrder = orderDetailsFromComplete(firstCompleteResult);

  const orderReadVariables = { id: completedOrder.id };
  const afterFirstOrderReadResult = await runGraphqlRequest<JsonRecord>(orderReadDocument, orderReadVariables);
  assertHttpOk('order read after first completion', afterFirstOrderReadResult);
  assertOrderReadMatches(afterFirstOrderReadResult, completedOrder.id);

  const secondCompleteResult = await runGraphqlRequest<JsonRecord>(completeDocument, completeVariables);
  assertHttpOk('second draftOrderComplete', secondCompleteResult);
  assertPaidError(secondCompleteResult);

  const afterSecondOrderReadResult = await runGraphqlRequest<JsonRecord>(orderReadDocument, orderReadVariables);
  assertHttpOk('order read after second completion', afterSecondOrderReadResult);
  assertOrderReadUnchanged(afterFirstOrderReadResult, afterSecondOrderReadResult);

  const afterSecondDraftReadResult = await runGraphqlRequest<JsonRecord>(draftOrderReadDocument, { id: draftOrderId });
  assertHttpOk('draft order read after second completion', afterSecondDraftReadResult);

  const cleanupVariables = {
    orderId: completedOrder.id,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  };
  const cleanupResult = await runGraphqlRequest<JsonRecord>(orderCancelDocument, cleanupVariables);

  await writeJson(fixturePath, {
    setup: {
      draftOrderCreate: {
        query: createDocument,
        variables: createVariables,
        response: createResult.payload,
      },
      firstComplete: {
        query: completeDocument,
        variables: completeVariables,
        response: firstCompleteResult.payload,
      },
      afterFirstOrderRead: {
        query: orderReadDocument,
        variables: orderReadVariables,
        response: afterFirstOrderReadResult.payload,
      },
    },
    mutation: {
      query: completeDocument,
      variables: completeVariables,
      response: secondCompleteResult.payload,
    },
    orderRead: {
      query: orderReadDocument,
      variables: orderReadVariables,
    },
    afterSecondOrderRead: {
      query: orderReadDocument,
      variables: orderReadVariables,
      response: afterSecondOrderReadResult.payload,
    },
    downstreamRead: {
      query: trimGraphql(draftOrderReadDocument),
      variables: { id: draftOrderId },
      response: afterSecondDraftReadResult.payload,
    },
    cleanup: {
      cancelOrder: {
        query: trimGraphql(orderCancelDocument),
        variables: cleanupVariables,
        response: cleanupResult.payload,
      },
    },
  });

  console.log(`Wrote ${fixturePath}`);
}

await main();
