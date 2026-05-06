/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type Capture = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

type CreatedOrder = {
  id: string;
  variables: JsonRecord;
  create: Capture;
};

type CreatedCustomer = {
  id: string;
  variables: JsonRecord;
  create: Capture;
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

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const requestDir = path.join('config', 'parity-requests', 'orders');
const createdOrders: CreatedOrder[] = [];
const createdCustomers: CreatedCustomer[] = [];
const cleanup: Record<string, unknown> = {};

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord): Promise<Capture> {
  return {
    query: trimGraphql(query),
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

function asRecord(value: unknown): JsonRecord {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    return {};
  }
  return value as JsonRecord;
}

function readRoot(captureResult: Capture, root: string): JsonRecord {
  return asRecord(asRecord(captureResult.response.payload.data)[root]);
}

function assertOk(label: string, captureResult: Capture): void {
  const payload = captureResult.response.payload;
  if (captureResult.response.status < 200 || captureResult.response.status >= 300 || payload['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(payload, null, 2)}`);
  }
}

function assertNoUserErrors(label: string, captureResult: Capture, root: string): void {
  const userErrors = readRoot(captureResult, root)['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
}

function assertAccessDenied(label: string, captureResult: Capture, root: string): void {
  const payload = captureResult.response.payload;
  const rootValue = asRecord(payload['data'])[root] ?? null;
  const errors = Array.isArray(payload['errors']) ? payload['errors'].map(asRecord) : [];
  if (rootValue === null && errors.some((error) => asRecord(error['extensions'])['code'] === 'ACCESS_DENIED')) {
    return;
  }
  throw new Error(`${label} did not return ACCESS_DENIED: ${JSON.stringify(payload, null, 2)}`);
}

const orderFields = `#graphql
  fragment OrderManagementFields on Order {
    id
    name
    closed
    closedAt
    cancelledAt
    cancelReason
    displayFinancialStatus
    paymentGatewayNames
    totalOutstandingSet {
      shopMoney {
        amount
        currencyCode
      }
    }
    currentTotalPriceSet {
      shopMoney {
        amount
        currencyCode
      }
    }
    customer {
      id
      email
      displayName
    }
    transactions {
      kind
      status
      gateway
      amountSet {
        shopMoney {
          amount
          currencyCode
        }
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  ${orderFields}
  mutation OrderManagementCreateOrder($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ...OrderManagementFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const customerCreateMutation = `#graphql
  mutation OrderManagementCreateCustomer($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        id
        email
        displayName
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCancelCleanupMutation = `#graphql
  mutation OrderManagementCleanupCancel(
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

const customerDeleteMutation = `#graphql
  mutation OrderManagementCleanupCustomer($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

const orderReadQuery = await readRequest('order-management-downstream-read.graphql');
const orderCancelMutation = await readRequest('orderCancel-parity.graphql');
const orderCloseMutation = await readRequest('orderClose-parity.graphql');
const orderOpenMutation = await readRequest('orderOpen-parity.graphql');
const orderCustomerSetMutation = await readRequest('orderCustomerSet-parity.graphql');
const orderCustomerRemoveMutation = await readRequest('orderCustomerRemove-parity.graphql');
const orderInvoiceSendMutation = await readRequest('orderInvoiceSend-parity.graphql');
const orderCreateManualPaymentMutation = await readRequest('orderCreateManualPayment-access-denied-parity.graphql');
const taxSummaryCreateMutation = await readRequest('taxSummaryCreate-access-denied-parity.graphql');

function orderCreateVariables(label: string, stamp: number, amount: string, email?: string): JsonRecord {
  return {
    order: {
      email,
      note: `shopify-draft-proxy ${label} order management capture`,
      tags: ['shopify-draft-proxy', 'order-management', label],
      test: true,
      currency: 'USD',
      lineItems: [
        {
          title: `Order management ${label} item`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount,
              currencyCode: 'USD',
            },
          },
          requiresShipping: false,
          taxable: false,
          sku: `sdp-order-management-${label}-${stamp}`,
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

async function createOrder(label: string, stamp: number, amount: string, email?: string): Promise<CreatedOrder> {
  const variables = orderCreateVariables(label, stamp, amount, email);
  const createResult = await capture(orderCreateMutation, variables);
  assertOk(`${label} orderCreate`, createResult);
  assertNoUserErrors(`${label} orderCreate`, createResult, 'orderCreate');
  const order = asRecord(readRoot(createResult, 'orderCreate')['order']);
  const id = String(order['id'] ?? '');
  if (!id) {
    throw new Error(`${label} orderCreate did not return an order id`);
  }
  const createdOrder = { id, variables, create: createResult };
  createdOrders.push(createdOrder);
  return createdOrder;
}

async function createCustomer(stamp: number): Promise<CreatedCustomer> {
  const variables = {
    input: {
      email: `sdp-order-management-${stamp}@example.com`,
      firstName: 'Order',
      lastName: 'Management',
      tags: ['shopify-draft-proxy', 'order-management'],
    },
  };
  const createResult = await capture(customerCreateMutation, variables);
  assertOk('customerCreate', createResult);
  assertNoUserErrors('customerCreate', createResult, 'customerCreate');
  const customer = asRecord(readRoot(createResult, 'customerCreate')['customer']);
  const id = String(customer['id'] ?? '');
  if (!id) {
    throw new Error('customerCreate did not return a customer id');
  }
  const createdCustomer = { id, variables, create: createResult };
  createdCustomers.push(createdCustomer);
  return createdCustomer;
}

async function readOrder(id: string): Promise<Capture> {
  const result = await capture(orderReadQuery, { id });
  assertOk('order read', result);
  return result;
}

function orderFromRead(read: Capture): JsonRecord {
  return asRecord(asRecord(read.response.payload['data'])['order']);
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function readOrderUntilCancelled(id: string): Promise<Capture> {
  let latest = await readOrder(id);
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const order = orderFromRead(latest);
    if (order['closed'] === true && order['cancelReason'] !== null) {
      return latest;
    }
    await delay(1500);
    latest = await readOrder(id);
  }
  throw new Error(
    `orderCancel downstream read did not reach cancelled state: ${JSON.stringify(latest.response.payload)}`,
  );
}

function orderHydrateCall(operationName: string, orderRead: Capture): JsonRecord {
  return {
    operationName,
    variables: orderRead.variables,
    query: orderRead.query,
    response: {
      status: orderRead.response.status,
      body: orderRead.response.payload,
    },
  };
}

function customerHydrateCall(customer: CreatedCustomer): JsonRecord {
  const customerPayload = asRecord(readRoot(customer.create, 'customerCreate')['customer']);
  return {
    operationName: 'CustomerHydrate',
    variables: { id: customer.id },
    query: 'query CustomerHydrate($id: ID!) { customer(id: $id) { id email displayName } }',
    response: {
      status: 200,
      body: {
        data: {
          customer: customerPayload,
        },
      },
    },
  };
}

async function writeFixture(name: string, payload: unknown): Promise<void> {
  await writeJson(path.join(fixtureDir, name), payload);
}

async function captureOrderClose(stamp: number): Promise<void> {
  const order = await createOrder('close', stamp, '12.00');
  const before = await readOrder(order.id);
  const variables = { input: { id: order.id } };
  const mutation = await capture(orderCloseMutation, variables);
  assertOk('orderClose', mutation);
  assertNoUserErrors('orderClose', mutation, 'orderClose');
  const downstreamRead = await readOrder(order.id);
  await writeFixture('orderClose-parity.json', {
    variables,
    mutation: { response: mutation.response.payload },
    downstreamRead: { variables: downstreamRead.variables, response: downstreamRead.response.payload },
    upstreamCalls: [orderHydrateCall('OrdersOrderHydrate', before)],
  });
}

async function captureOrderOpen(stamp: number): Promise<void> {
  const order = await createOrder('open', stamp, '13.00');
  const closeVariables = { input: { id: order.id } };
  const close = await capture(orderCloseMutation, closeVariables);
  assertOk('setup orderClose', close);
  assertNoUserErrors('setup orderClose', close, 'orderClose');
  const before = await readOrder(order.id);
  const variables = { input: { id: order.id } };
  const mutation = await capture(orderOpenMutation, variables);
  assertOk('orderOpen', mutation);
  assertNoUserErrors('orderOpen', mutation, 'orderOpen');
  const downstreamRead = await readOrder(order.id);
  await writeFixture('orderOpen-parity.json', {
    variables,
    mutation: { response: mutation.response.payload },
    downstreamRead: { variables: downstreamRead.variables, response: downstreamRead.response.payload },
    upstreamCalls: [orderHydrateCall('OrdersOrderHydrate', before)],
  });
}

async function captureOrderCancel(stamp: number): Promise<void> {
  const order = await createOrder('cancel', stamp, '14.00');
  const before = await readOrder(order.id);
  const variables = {
    orderId: order.id,
    restock: false,
    reason: 'OTHER',
    notifyCustomer: false,
    staffNote: 'shopify-draft-proxy order cancel capture',
  };
  const mutation = await capture(orderCancelMutation, variables);
  assertOk('orderCancel', mutation);
  const downstreamRead = await readOrderUntilCancelled(order.id);
  await writeFixture('orderCancel-parity.json', {
    variables,
    mutation: { response: mutation.response.payload },
    downstreamRead: { variables: downstreamRead.variables, response: downstreamRead.response.payload },
    upstreamCalls: [orderHydrateCall('OrdersOrderHydrate', before)],
  });
}

async function captureOrderCustomer(stamp: number): Promise<void> {
  const order = await createOrder('customer', stamp, '15.00');
  const customer = await createCustomer(stamp);
  const beforeSet = await readOrder(order.id);
  const setVariables = { orderId: order.id, customerId: customer.id };
  const setMutation = await capture(orderCustomerSetMutation, setVariables);
  assertOk('orderCustomerSet', setMutation);
  assertNoUserErrors('orderCustomerSet', setMutation, 'orderCustomerSet');
  const afterSet = await readOrder(order.id);

  await writeFixture('orderCustomerSet-parity.json', {
    variables: setVariables,
    mutation: { response: setMutation.response.payload },
    downstreamRead: { variables: afterSet.variables, response: afterSet.response.payload },
    upstreamCalls: [orderHydrateCall('CustomerOrderSummaryHydrate', beforeSet), customerHydrateCall(customer)],
  });

  const removeVariables = { orderId: order.id };
  const removeMutation = await capture(orderCustomerRemoveMutation, removeVariables);
  assertOk('orderCustomerRemove', removeMutation);
  assertNoUserErrors('orderCustomerRemove', removeMutation, 'orderCustomerRemove');
  const afterRemove = await readOrder(order.id);

  await writeFixture('orderCustomerRemove-parity.json', {
    variables: removeVariables,
    mutation: { response: removeMutation.response.payload },
    downstreamRead: { variables: afterRemove.variables, response: afterRemove.response.payload },
    upstreamCalls: [orderHydrateCall('CustomerOrderSummaryHydrate', afterSet)],
  });
}

async function captureOrderInvoiceSend(stamp: number): Promise<void> {
  const email = `sdp-order-invoice-${stamp}@example.com`;
  const order = await createOrder('invoice', stamp, '16.00', email);
  const before = await readOrder(order.id);
  const variables = {
    id: order.id,
    email: {
      to: email,
      subject: 'Shopify draft proxy conformance invoice',
      customMessage: 'Order invoice send capture for shopify-draft-proxy conformance.',
    },
  };
  const mutation = await capture(orderInvoiceSendMutation, variables);
  assertOk('orderInvoiceSend', mutation);
  assertNoUserErrors('orderInvoiceSend', mutation, 'orderInvoiceSend');
  const downstreamRead = await readOrder(order.id);
  await writeFixture('orderInvoiceSend-parity.json', {
    variables,
    mutation: { response: mutation.response.payload },
    downstreamRead: { variables: downstreamRead.variables, response: downstreamRead.response.payload },
    upstreamCalls: [orderHydrateCall('OrdersOrderHydrate', before)],
  });
}

async function captureAccessDenied(): Promise<void> {
  const manualPaymentVariables = {
    id: 'gid://shopify/Order/0',
    amount: {
      amount: '16.00',
      currencyCode: 'USD',
    },
    paymentMethodName: 'Shopify draft proxy manual payment',
    processedAt: new Date().toISOString(),
  };
  const manualPayment = await capture(orderCreateManualPaymentMutation, manualPaymentVariables);
  assertAccessDenied('orderCreateManualPayment', manualPayment, 'orderCreateManualPayment');
  await writeFixture('orderCreateManualPayment-access-denied-parity.json', {
    variables: manualPaymentVariables,
    mutation: { response: manualPayment.response.payload },
    upstreamCalls: [],
  });

  const now = Date.now();
  const taxVariables = {
    orderId: 'gid://shopify/Order/0',
    startTime: new Date(now - 60 * 60 * 1000).toISOString(),
    endTime: new Date(now).toISOString(),
  };
  const taxSummary = await capture(taxSummaryCreateMutation, taxVariables);
  assertAccessDenied('taxSummaryCreate', taxSummary, 'taxSummaryCreate');
  await writeFixture('taxSummaryCreate-access-denied-parity.json', {
    variables: taxVariables,
    mutation: { response: taxSummary.response.payload },
    upstreamCalls: [
      {
        operationName: 'TaxSummaryCreateAccessDenied',
        variables: taxVariables,
        query: trimGraphql(taxSummaryCreateMutation),
        response: {
          status: taxSummary.response.status,
          body: taxSummary.response.payload,
        },
      },
    ],
  });
}

async function cleanupOrder(orderId: string): Promise<unknown> {
  const variables = {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  };
  const result = await capture(orderCancelCleanupMutation, variables);
  return {
    query: result.query,
    variables,
    response: result.response.payload,
  };
}

async function cleanupCustomer(customerId: string): Promise<unknown> {
  const variables = { input: { id: customerId } };
  const result = await capture(customerDeleteMutation, variables);
  return {
    query: result.query,
    variables,
    response: result.response.payload,
  };
}

const stamp = Date.now();

try {
  await captureOrderClose(stamp);
  await captureOrderOpen(stamp);
  await captureOrderCancel(stamp);
  await captureOrderCustomer(stamp);
  await captureOrderInvoiceSend(stamp);
  const accessDeniedOrder = createdOrders[createdOrders.length - 1];
  if (!accessDeniedOrder) {
    throw new Error('No disposable order was available for access-denied probes');
  }
  await captureAccessDenied();
} finally {
  for (const order of createdOrders) {
    try {
      cleanup[`order:${order.id}`] = await cleanupOrder(order.id);
    } catch (error) {
      cleanup[`order:${order.id}`] = { error: error instanceof Error ? error.message : String(error) };
    }
  }
  for (const customer of createdCustomers) {
    try {
      cleanup[`customer:${customer.id}`] = await cleanupCustomer(customer.id);
    } catch (error) {
      cleanup[`customer:${customer.id}`] = { error: error instanceof Error ? error.message : String(error) };
    }
  }
  if (createdOrders.length > 0 || createdCustomers.length > 0) {
    await writeFixture('order-management-cleanup.json', {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cleanup,
    });
  }
}

console.log(
  JSON.stringify(
    {
      ok: true,
      outputDir: fixtureDir,
      orders: createdOrders.map((order) => order.id),
      customers: createdCustomers.map((customer) => customer.id),
    },
    null,
    2,
  ),
);
