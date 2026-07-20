/* oxlint-disable no-console -- CLI capture scripts intentionally report live capture status. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type GraphqlCapture = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

const config = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  env: { ...process.env, SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
  exitOnMissing: true,
});
const token = await getValidConformanceAccessToken({
  adminOrigin: config.adminOrigin,
  apiVersion: config.apiVersion,
});
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin: config.adminOrigin,
  apiVersion: config.apiVersion,
  headers: buildAdminAuthHeaders(token),
});
const outputPath = path.join(
  'fixtures',
  'conformance',
  config.storeDomain,
  config.apiVersion,
  'payments',
  'payment-terms-cold-delete.json',
);

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

function record(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function array(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function requireId(value: unknown, context: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${context} did not return an id: ${JSON.stringify(value, null, 2)}`);
  }
  return value;
}

function root(capture: GraphqlCapture, name: string): JsonRecord | null {
  return record(record(capture.response.payload.data)?.[name]);
}

function assertGraphqlSuccess(capture: GraphqlCapture, context: string): void {
  if (capture.response.status < 200 || capture.response.status >= 300 || capture.response.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(capture.response, null, 2)}`);
  }
}

function assertMutationSuccess(capture: GraphqlCapture, rootName: string, context: string): void {
  assertGraphqlSuccess(capture, context);
  const errors = array(root(capture, rootName)?.['userErrors']);
  if (errors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

async function run(query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  const cleanQuery = trimGraphql(query);
  return {
    query: cleanQuery,
    variables,
    response: await runGraphqlRequest<JsonRecord>(cleanQuery, variables),
  };
}

function payload(capture: GraphqlCapture): JsonRecord {
  return {
    query: capture.query,
    variables: capture.variables,
    response: capture.response.payload,
  };
}

function upstreamCall(capture: GraphqlCapture): JsonRecord {
  return {
    operationName: 'PaymentTermsHydrate',
    variables: capture.variables,
    query: capture.query,
    response: {
      status: capture.response.status,
      body: capture.response.payload,
    },
  };
}

const orderCreateDocument = await readFile(
  'config/parity-requests/payments/payment-terms-create-on-order-create.graphql',
  'utf8',
);
const paymentTermsCreateDocument = await readFile(
  'config/parity-requests/payments/payment-terms-lifecycle-create.graphql',
  'utf8',
);
const paymentTermsDeleteDocument = await readFile(
  'config/parity-requests/payments/payment-terms-lifecycle-delete.graphql',
  'utf8',
);
const orderReadDocument = await readFile(
  'config/parity-requests/payments/payment-terms-owner-cascade-order-read.graphql',
  'utf8',
);
const draftReadDocument = await readFile(
  'config/parity-requests/payments/payment-terms-owner-cascade-draft-read.graphql',
  'utf8',
);
const nodeReadDocument = await readFile(
  'config/parity-requests/payments/payment-terms-cold-delete-node-read.graphql',
  'utf8',
);

const paymentTermsHydrateDocument =
  'query PaymentTermsHydrate($id: ID!) {\n    paymentTerms: node(id: $id) {\n      ... on PaymentTerms {\n        id\n        due\n        overdue\n        dueInDays\n        paymentTermsName\n        paymentTermsType\n        translatedName\n        order {\n          id\n          name\n          email\n          closed\n          closedAt\n          cancelledAt\n          displayFinancialStatus\n          totalOutstandingSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n          currentTotalPriceSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n          totalPriceSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n          lineItems(first: 1) {\n            nodes {\n              sellingPlan {\n                name\n              }\n            }\n          }\n        }\n        draftOrder {\n          id\n          name\n          status\n          completedAt\n          subtotalPriceSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n          totalPriceSet {\n            shopMoney { amount currencyCode }\n            presentmentMoney { amount currencyCode }\n          }\n        }\n        paymentSchedules(first: 10) {\n          nodes {\n            id\n            dueAt\n            issuedAt\n            completedAt\n            due\n            amount { amount currencyCode }\n            balanceDue { amount currencyCode }\n            totalBalance { amount currencyCode }\n          }\n        }\n      }\n    }\n  }';

const draftOrderCreateDocument = `#graphql
  mutation PaymentTermsColdDeleteDraftCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder { id name }
      userErrors { field message }
    }
  }
`;
const draftOrderDeleteDocument = `#graphql
  mutation PaymentTermsColdDeleteDraftCleanup($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors { field message }
    }
  }
`;
const orderCancelDocument = `#graphql
  mutation PaymentTermsColdDeleteOrderCleanup(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(
      orderId: $orderId
      reason: $reason
      notifyCustomer: $notifyCustomer
      restock: $restock
    ) {
      job { id done }
      orderCancelUserErrors { field message code }
      userErrors { field message }
    }
  }
`;

const stamp = Date.now();
const paymentTermsAttributes = {
  paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
  paymentSchedules: [{ issuedAt: new Date().toISOString() }],
};
const cleanup: JsonRecord = {};
let orderId: string | null = null;
let draftOrderId: string | null = null;
let fixture: JsonRecord | null = null;

try {
  const orderCreate = await run(orderCreateDocument, {
    order: {
      email: `payment-terms-cold-delete-order-${stamp}@example.com`,
      currency: 'USD',
      presentmentCurrency: 'USD',
      test: true,
      lineItems: [
        {
          title: 'Payment terms cold delete order',
          quantity: 1,
          priceSet: {
            shopMoney: { amount: '42.50', currencyCode: 'USD' },
            presentmentMoney: { amount: '42.50', currencyCode: 'USD' },
          },
          requiresShipping: false,
          taxable: false,
        },
      ],
    },
  });
  assertMutationSuccess(orderCreate, 'orderCreate', 'cold delete orderCreate');
  orderId = requireId(record(root(orderCreate, 'orderCreate')?.['order'])?.['id'], 'cold delete orderCreate');

  const orderTermsCreate = await run(paymentTermsCreateDocument, {
    referenceId: orderId,
    attrs: paymentTermsAttributes,
  });
  assertMutationSuccess(orderTermsCreate, 'paymentTermsCreate', 'cold delete order terms create');
  const orderTerms = record(root(orderTermsCreate, 'paymentTermsCreate')?.['paymentTerms']);
  const orderTermsId = requireId(orderTerms?.['id'], 'cold delete order terms');
  const orderScheduleId = requireId(
    record(array(record(orderTerms?.['paymentSchedules'])?.['nodes'])[0])?.['id'],
    'cold delete order schedule',
  );
  const orderHydrate = await run(paymentTermsHydrateDocument, { id: orderTermsId });
  assertGraphqlSuccess(orderHydrate, 'cold delete order hydrate');
  const orderDelete = await run(paymentTermsDeleteDocument, { input: { paymentTermsId: orderTermsId } });
  assertMutationSuccess(orderDelete, 'paymentTermsDelete', 'cold delete order delete');
  const orderRead = await run(orderReadDocument, { id: orderId });
  const orderTermsNodeRead = await run(nodeReadDocument, { id: orderTermsId });
  const orderScheduleNodeRead = await run(nodeReadDocument, { id: orderScheduleId });
  assertGraphqlSuccess(orderRead, 'cold delete order read');
  assertGraphqlSuccess(orderTermsNodeRead, 'cold delete order terms node read');
  assertGraphqlSuccess(orderScheduleNodeRead, 'cold delete order schedule node read');

  const draftCreate = await run(draftOrderCreateDocument, {
    input: {
      email: `payment-terms-cold-delete-draft-${stamp}@example.com`,
      note: 'Payment terms cold delete draft',
      presentmentCurrencyCode: 'CAD',
      lineItems: [
        {
          title: 'Payment terms cold delete draft',
          quantity: 1,
          originalUnitPriceWithCurrency: { amount: '18.50', currencyCode: 'CAD' },
          requiresShipping: false,
          taxable: false,
        },
      ],
    },
  });
  assertMutationSuccess(draftCreate, 'draftOrderCreate', 'cold delete draftOrderCreate');
  const draft = record(root(draftCreate, 'draftOrderCreate')?.['draftOrder']);
  draftOrderId = requireId(draft?.['id'], 'cold delete draftOrderCreate');
  const draftTermsCreate = await run(paymentTermsCreateDocument, {
    referenceId: draftOrderId,
    attrs: paymentTermsAttributes,
  });
  assertMutationSuccess(draftTermsCreate, 'paymentTermsCreate', 'cold delete draft terms create');
  const draftTerms = record(root(draftTermsCreate, 'paymentTermsCreate')?.['paymentTerms']);
  const draftTermsId = requireId(draftTerms?.['id'], 'cold delete draft terms');
  const draftScheduleId = requireId(
    record(array(record(draftTerms?.['paymentSchedules'])?.['nodes'])[0])?.['id'],
    'cold delete draft schedule',
  );
  const draftHydrate = await run(paymentTermsHydrateDocument, { id: draftTermsId });
  assertGraphqlSuccess(draftHydrate, 'cold delete draft hydrate');
  const draftDelete = await run(paymentTermsDeleteDocument, { input: { paymentTermsId: draftTermsId } });
  assertMutationSuccess(draftDelete, 'paymentTermsDelete', 'cold delete draft delete');
  const draftRead = await run(draftReadDocument, { id: draftOrderId });
  const draftTermsNodeRead = await run(nodeReadDocument, { id: draftTermsId });
  const draftScheduleNodeRead = await run(nodeReadDocument, { id: draftScheduleId });
  assertGraphqlSuccess(draftRead, 'cold delete draft read');
  assertGraphqlSuccess(draftTermsNodeRead, 'cold delete draft terms node read');
  assertGraphqlSuccess(draftScheduleNodeRead, 'cold delete draft schedule node read');

  const missingTermsId = 'gid://shopify/PaymentTerms/999999999999999';
  const missingHydrate = await run(paymentTermsHydrateDocument, { id: missingTermsId });
  assertGraphqlSuccess(missingHydrate, 'cold delete missing hydrate');
  const missingDelete = await run(paymentTermsDeleteDocument, {
    input: { paymentTermsId: missingTermsId },
  });
  assertGraphqlSuccess(missingDelete, 'cold delete missing delete');

  fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain: config.storeDomain,
    apiVersion: config.apiVersion,
    notes:
      'Live Shopify capture for mutation-first paymentTermsDelete against Order-owned and DraftOrder-owned terms. Exact query-only node hydrates are recorded before the live deletes; downstream owner and generic Node reads are captured after deletion.',
    order: {
      ownerId: orderId,
      paymentTermsId: orderTermsId,
      paymentScheduleId: orderScheduleId,
      deleteVariables: orderDelete.variables,
      expected: {
        delete: orderDelete.response.payload,
        ownerRead: orderRead.response.payload,
        paymentTermsNodeRead: orderTermsNodeRead.response.payload,
        paymentScheduleNodeRead: orderScheduleNodeRead.response.payload,
      },
    },
    draft: {
      ownerId: draftOrderId,
      paymentTermsId: draftTermsId,
      paymentScheduleId: draftScheduleId,
      deleteVariables: draftDelete.variables,
      expected: {
        delete: draftDelete.response.payload,
        ownerRead: draftRead.response.payload,
        paymentTermsNodeRead: draftTermsNodeRead.response.payload,
        paymentScheduleNodeRead: draftScheduleNodeRead.response.payload,
      },
    },
    missing: {
      paymentTermsId: missingTermsId,
      deleteVariables: missingDelete.variables,
      expected: { delete: missingDelete.response.payload },
    },
    upstreamCalls: [upstreamCall(orderHydrate), upstreamCall(draftHydrate), upstreamCall(missingHydrate)],
    cleanup,
  };
} finally {
  if (draftOrderId) {
    cleanup['draftOrderDelete'] = payload(await run(draftOrderDeleteDocument, { input: { id: draftOrderId } }));
  }
  if (orderId) {
    cleanup['orderCancel'] = payload(
      await run(orderCancelDocument, {
        orderId,
        reason: 'OTHER',
        notifyCustomer: false,
        restock: false,
      }),
    );
  }
}

if (!fixture) throw new Error('payment terms cold delete fixture was not captured');
await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
