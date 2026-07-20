/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

type CreatedOrder = {
  id: string;
  create: {
    query: string;
    variables: JsonRecord;
    response: ConformanceGraphqlPayload<JsonRecord>;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'payments');
const outputPath = path.join(outputDir, 'payment-terms-update-order-eligibility.json');
const cleanupPath = path.join(outputDir, 'payment-terms-update-order-eligibility-cleanup.json');

const orderCreateDocument = `#graphql
  mutation PaymentTermsUpdateEligibilityOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFinancialStatus
        closed
        closedAt
        cancelledAt
        totalOutstandingSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
        currentTotalPriceSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
        totalPriceSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderMarkAsPaidDocument = `#graphql
  mutation PaymentTermsUpdateEligibilityOrderMarkAsPaid($input: OrderMarkAsPaidInput!) {
    orderMarkAsPaid(input: $input) {
      order {
        id
        displayFinancialStatus
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCancelDocument = `#graphql
  mutation PaymentTermsUpdateEligibilityOrderCancel(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job {
        id
        done
      }
      orderCancelUserErrors {
        field
        message
        code
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const paymentTermsCreateDocument = `#graphql
  mutation PaymentTermsUpdateEligibilityCreate($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
    paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
      paymentTerms {
        id
        due
        overdue
        dueInDays
        paymentTermsName
        paymentTermsType
        translatedName
        paymentSchedules(first: 2) {
          nodes {
            id
            issuedAt
            dueAt
            completedAt
            due
            amount {
              amount
              currencyCode
            }
            balanceDue {
              amount
              currencyCode
            }
            totalBalance {
              amount
              currencyCode
            }
          }
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const paymentTermsHydrateDocument = `#graphql
  query PaymentTermsHydrate($id: ID!) {
    paymentTerms: node(id: $id) {
      ... on PaymentTerms {
        id
        due
        overdue
        dueInDays
        paymentTermsName
        paymentTermsType
        translatedName
        order {
          id
          name
          email
          closed
          closedAt
          cancelledAt
          displayFinancialStatus
          totalOutstandingSet {
            shopMoney { amount currencyCode }
            presentmentMoney { amount currencyCode }
          }
          currentTotalPriceSet {
            shopMoney { amount currencyCode }
            presentmentMoney { amount currencyCode }
          }
          totalPriceSet {
            shopMoney { amount currencyCode }
            presentmentMoney { amount currencyCode }
          }
          lineItems(first: 1) {
            nodes {
              sellingPlan {
                name
              }
            }
          }
        }
        draftOrder {
          id
          name
          status
          completedAt
          subtotalPriceSet {
            shopMoney { amount currencyCode }
            presentmentMoney { amount currencyCode }
          }
          totalPriceSet {
            shopMoney { amount currencyCode }
            presentmentMoney { amount currencyCode }
          }
        }
        paymentSchedules(first: 10) {
          nodes {
            id
            dueAt
            issuedAt
            completedAt
            due
            amount { amount currencyCode }
            balanceDue { amount currencyCode }
            totalBalance { amount currencyCode }
          }
        }
      }
    }
  }
`;

const paymentTermsUpdateDocument = `#graphql
  mutation PaymentTermsUpdateOrderEligibility($input: PaymentTermsUpdateInput!) {
    paymentTermsUpdate(input: $input) {
      paymentTerms {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const paymentTermsDeleteDocument = `#graphql
  mutation PaymentTermsUpdateEligibilityTermsCleanup($input: PaymentTermsDeleteInput!) {
    paymentTermsDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const defaultPaymentTermsAttrs = {
  paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
  paymentSchedules: [{ issuedAt: '2026-05-05T00:00:00Z' }],
};

const updateAttrs = {
  paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/7',
  paymentSchedules: [{ dueAt: '2026-06-05T00:00:00Z' }],
};

const paidMessage = 'Cannot create payment terms on an Order that has already been paid in full.';

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
}

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readRecord(value: unknown, key: string): JsonRecord | null {
  return asRecord(asRecord(value)?.[key]);
}

function readArray(value: unknown, key: string): unknown[] {
  const fieldValue = asRecord(value)?.[key];
  return Array.isArray(fieldValue) ? fieldValue : [];
}

function readString(value: unknown, key: string): string | null {
  const fieldValue = asRecord(value)?.[key];
  return typeof fieldValue === 'string' && fieldValue.length > 0 ? fieldValue : null;
}

async function run(query: string, variables: JsonRecord): Promise<GraphqlCapture> {
  const cleanQuery = trimGraphql(query);
  return {
    query: cleanQuery,
    variables,
    response: await runGraphqlRequest<JsonRecord>(cleanQuery, variables),
  };
}

function assertOk(label: string, capture: GraphqlCapture): void {
  const payload = capture.response.payload;
  if (capture.response.status < 200 || capture.response.status >= 300 || payload['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(payload, null, 2)}`);
  }
}

function assertNoUserErrors(label: string, capture: GraphqlCapture, root: string): void {
  assertOk(label, capture);
  const payload = readRecord(capture.response.payload['data'], root);
  const errors = readArray(payload, 'userErrors');
  const cancelErrors = readArray(payload, 'orderCancelUserErrors');
  if (errors.length === 0 && cancelErrors.length === 0) {
    return;
  }
  throw new Error(`${label} returned user errors: ${JSON.stringify({ errors, cancelErrors }, null, 2)}`);
}

function paymentTermsCreatePayload(capture: GraphqlCapture): JsonRecord | null {
  return readRecord(capture.response.payload['data'], 'paymentTermsCreate');
}

function paymentTermsUpdatePayload(capture: GraphqlCapture): JsonRecord | null {
  return readRecord(capture.response.payload['data'], 'paymentTermsUpdate');
}

function requireCreatedPaymentTermsId(label: string, capture: GraphqlCapture): string {
  assertNoUserErrors(label, capture, 'paymentTermsCreate');
  const payload = paymentTermsCreatePayload(capture);
  const terms = asRecord(payload?.['paymentTerms']);
  const id = readString(terms, 'id');
  if (!id) {
    throw new Error(`${label} did not create payment terms: ${JSON.stringify(payload, null, 2)}`);
  }
  return id;
}

function assertPaymentTermsUpdateRejected(label: string, capture: GraphqlCapture): void {
  assertOk(label, capture);
  const payload = paymentTermsUpdatePayload(capture);
  const terms = asRecord(payload?.['paymentTerms']);
  const errors = readArray(payload, 'userErrors').map(asRecord);
  const firstError = errors[0] ?? null;
  if (
    terms !== null ||
    errors.length !== 1 ||
    firstError?.['field'] !== null ||
    firstError?.['code'] !== 'PAYMENT_TERMS_UPDATE_UNSUCCESSFUL' ||
    firstError?.['message'] !== paidMessage
  ) {
    throw new Error(`${label} did not match expected rejection: ${JSON.stringify(payload, null, 2)}`);
  }
}

function orderVariables(stamp: number): JsonRecord {
  const amount = '12.50';
  const priceSet = {
    shopMoney: { amount, currencyCode: 'USD' },
    presentmentMoney: { amount, currencyCode: 'USD' },
  };
  return {
    order: {
      email: `payment-terms-update-paid-${stamp}@example.com`,
      note: 'payment terms update eligibility paid capture',
      tags: ['shopify-draft-proxy', 'payment-terms-update-eligibility', 'paid'],
      test: true,
      currency: 'USD',
      presentmentCurrency: 'USD',
      lineItems: [
        {
          title: 'Payment terms update eligibility paid',
          quantity: 1,
          priceSet,
          requiresShipping: false,
          taxable: false,
          sku: `sdp-payment-terms-update-paid-${stamp}`,
        },
      ],
    },
    options: {
      inventoryBehaviour: 'BYPASS',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  };
}

async function createOrder(stamp: number): Promise<CreatedOrder> {
  const variables = orderVariables(stamp);
  const create = await run(orderCreateDocument, variables);
  assertNoUserErrors('orderCreate setup', create, 'orderCreate');
  const order = readRecord(readRecord(create.response.payload['data'], 'orderCreate'), 'order');
  const id = readString(order, 'id');
  if (!id) {
    throw new Error(`orderCreate did not return an id: ${JSON.stringify(create.response.payload)}`);
  }
  return {
    id,
    create: {
      query: create.query,
      variables,
      response: create.response.payload,
    },
  };
}

async function markAsPaid(orderId: string): Promise<GraphqlCapture> {
  const capture = await run(orderMarkAsPaidDocument, { input: { id: orderId } });
  assertNoUserErrors('orderMarkAsPaid setup', capture, 'orderMarkAsPaid');
  return capture;
}

async function createPaymentTerms(orderId: string): Promise<GraphqlCapture> {
  return run(paymentTermsCreateDocument, {
    referenceId: orderId,
    attrs: defaultPaymentTermsAttrs,
  });
}

async function hydratePaymentTerms(paymentTermsId: string): Promise<GraphqlCapture> {
  const hydrate = await run(paymentTermsHydrateDocument, { id: paymentTermsId });
  assertOk('PaymentTermsHydrate', hydrate);
  return hydrate;
}

async function hydratePaymentTermsUntilPaid(paymentTermsId: string): Promise<GraphqlCapture> {
  let latest = await hydratePaymentTerms(paymentTermsId);
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const terms = readRecord(latest.response.payload['data'], 'paymentTerms');
    const order = readRecord(terms, 'order');
    if (order?.['displayFinancialStatus'] === 'PAID') {
      return latest;
    }
    await new Promise((resolve) => {
      setTimeout(resolve, 1500);
    });
    latest = await hydratePaymentTerms(paymentTermsId);
  }
  throw new Error(`PaymentTermsHydrate did not observe a PAID order: ${JSON.stringify(latest.response.payload)}`);
}

async function updatePaymentTerms(paymentTermsId: string): Promise<GraphqlCapture> {
  return run(paymentTermsUpdateDocument, {
    input: {
      paymentTermsId,
      paymentTermsAttributes: updateAttrs,
    },
  });
}

async function deletePaymentTerms(paymentTermsId: string): Promise<GraphqlCapture> {
  return run(paymentTermsDeleteDocument, { input: { paymentTermsId } });
}

async function cancelOrder(orderId: string): Promise<GraphqlCapture> {
  return run(orderCancelDocument, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
}

function capturePayload(capture: GraphqlCapture): JsonRecord {
  return {
    query: capture.query,
    variables: capture.variables,
    response: capture.response.payload,
  };
}

function upstreamCallFromHydrate(hydrate: GraphqlCapture): JsonRecord {
  return {
    operationName: 'PaymentTermsHydrate',
    variables: hydrate.variables,
    query: hydrate.query,
    response: {
      status: hydrate.response.status,
      body: hydrate.response.payload,
    },
  };
}

const stamp = Date.now();
let createdOrder: CreatedOrder | null = null;
let paymentTermsId: string | null = null;
const cleanup: JsonRecord = {};

try {
  createdOrder = await createOrder(stamp);
  const paymentTermsCreate = await createPaymentTerms(createdOrder.id);
  paymentTermsId = requireCreatedPaymentTermsId('paymentTermsCreate setup', paymentTermsCreate);
  const orderMarkAsPaid = await markAsPaid(createdOrder.id);
  const hydrate = await hydratePaymentTermsUntilPaid(paymentTermsId);
  const paymentTermsUpdate = await updatePaymentTerms(paymentTermsId);
  assertPaymentTermsUpdateRejected('paid paymentTermsUpdate', paymentTermsUpdate);

  await writeJson(outputPath, {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    cases: {
      paid: {
        setup: {
          orderCreate: createdOrder.create,
          paymentTermsCreate: capturePayload(paymentTermsCreate),
          orderMarkAsPaid: capturePayload(orderMarkAsPaid),
        },
        hydrate: capturePayload(hydrate),
        query: paymentTermsUpdate.query,
        variables: paymentTermsUpdate.variables,
        response: paymentTermsUpdate.response.payload,
      },
    },
    upstreamCalls: [upstreamCallFromHydrate(hydrate)],
    notes:
      'Captured against a disposable Shopify test Order. Shopify allowed payment terms on the unpaid Order, then rejected paymentTermsUpdate after orderMarkAsPaid made the owner fully paid.',
  });

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        orderId: createdOrder.id,
        paymentTermsId,
        userErrors: readArray(paymentTermsUpdatePayload(paymentTermsUpdate), 'userErrors'),
      },
      null,
      2,
    ),
  );
} finally {
  if (paymentTermsId) {
    try {
      cleanup['paymentTermsDelete'] = capturePayload(await deletePaymentTerms(paymentTermsId));
    } catch (error) {
      cleanup['paymentTermsDelete'] = {
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }
  if (createdOrder) {
    try {
      cleanup['orderCancel'] = capturePayload(await cancelOrder(createdOrder.id));
    } catch (error) {
      cleanup['orderCancel'] = {
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }
  if (Object.keys(cleanup).length > 0) {
    await writeJson(cleanupPath, {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cleanup,
    });
  }
}
