/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const customerCreateDocument = await readFile(
  'config/parity-requests/customers/customer-delete-order-precondition-customer-create.graphql',
  'utf8',
);
const orderCreateDocument = await readFile(
  'config/parity-requests/customers/customer-delete-order-precondition-order-create.graphql',
  'utf8',
);
const customerDeleteDocument = await readFile(
  'config/parity-requests/customers/customer-delete-order-precondition-delete.graphql',
  'utf8',
);
const customerReadDocument = await readFile(
  'config/parity-requests/customers/customer-delete-order-precondition-read.graphql',
  'utf8',
);

const orderCancelDocument = `#graphql
  mutation CustomerDeleteOrderPreconditionOrderCancel($orderId: ID!) {
    orderCancel(orderId: $orderId, reason: OTHER, notifyCustomer: false, restock: true) {
      job { id done }
      orderCancelUserErrors { field message code }
      userErrors { field message }
    }
  }
`;

const orderDeleteDocument = `#graphql
  mutation CustomerDeleteOrderPreconditionOrderDelete($orderId: ID!) {
    orderDelete(orderId: $orderId) {
      deletedId
      userErrors { field message code }
    }
  }
`;

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readCustomerCreateId(result: ConformanceGraphqlResult, context: string): string {
  const data = readRecord(result.payload.data);
  const customerCreate = readRecord(data?.['customerCreate']);
  const customer = readRecord(customerCreate?.['customer']);
  const id = customer?.['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${context} did not return customerCreate.customer.id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return id;
}

function readOrderCreateId(result: ConformanceGraphqlResult, context: string): string {
  const data = readRecord(result.payload.data);
  const orderCreate = readRecord(data?.['orderCreate']);
  const order = readRecord(orderCreate?.['order']);
  const id = order?.['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${context} did not return orderCreate.order.id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return id;
}

async function run(
  document: string,
  variables: Record<string, unknown>,
  context: string,
): Promise<ConformanceGraphqlResult> {
  const result = await runGraphqlRequest(document, variables);
  assertNoTopLevelErrors(result, context);
  return result;
}

async function bestEffortCleanup(customerId: string | null, orderId: string | null): Promise<Record<string, unknown>> {
  const cleanup: Record<string, unknown> = {};
  if (orderId) {
    const cancel = await runGraphqlRequest(orderCancelDocument, { orderId });
    cleanup['orderCancel'] = {
      operationName: 'CustomerDeleteOrderPreconditionOrderCancel',
      query: orderCancelDocument,
      variables: { orderId },
      response: cancel.payload,
    };
    const deleteOrder = await runGraphqlRequest(orderDeleteDocument, { orderId });
    cleanup['orderDelete'] = {
      operationName: 'CustomerDeleteOrderPreconditionOrderDelete',
      query: orderDeleteDocument,
      variables: { orderId },
      response: deleteOrder.payload,
    };
  }
  if (customerId) {
    const deleteCustomer = await runGraphqlRequest(customerDeleteDocument, { input: { id: customerId } });
    cleanup['customerDelete'] = {
      operationName: 'CustomerDeleteOrderPreconditionDelete',
      query: customerDeleteDocument,
      variables: { input: { id: customerId } },
      response: deleteCustomer.payload,
    };
  }
  return cleanup;
}

await mkdir(outputDir, { recursive: true });

const stamp = Date.now();

const controlCustomerVariables = {
  input: {
    email: `hermes-delete-control-${stamp}@example.com`,
    firstName: 'Control',
    lastName: 'Delete',
  },
};
const controlCustomerCreate = await run(customerCreateDocument, controlCustomerVariables, 'control customerCreate');
const controlCustomerId = readCustomerCreateId(controlCustomerCreate, 'control customerCreate');
const controlDelete = await run(customerDeleteDocument, { input: { id: controlCustomerId } }, 'control customerDelete');
const controlRead = await run(customerReadDocument, { id: controlCustomerId }, 'control read after customerDelete');

const blockedCustomerVariables = {
  input: {
    email: `hermes-delete-blocked-${stamp}@example.com`,
    firstName: 'Blocked',
    lastName: 'Delete',
  },
};
let blockedCustomerId: string | null = null;
let blockingOrderId: string | null = null;

try {
  const blockedCustomerCreate = await run(customerCreateDocument, blockedCustomerVariables, 'blocked customerCreate');
  blockedCustomerId = readCustomerCreateId(blockedCustomerCreate, 'blocked customerCreate');

  const orderVariables = {
    order: {
      email: blockedCustomerVariables.input.email,
      customerId: blockedCustomerId,
      currency: 'CAD',
      lineItems: [
        {
          title: 'Customer delete blocking line',
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '9.99',
              currencyCode: 'CAD',
            },
          },
        },
      ],
    },
  };

  const orderCreate = await run(orderCreateDocument, orderVariables, 'blocked orderCreate');
  blockingOrderId = readOrderCreateId(orderCreate, 'blocked orderCreate');
  const blockedDelete = await run(
    customerDeleteDocument,
    { input: { id: blockedCustomerId } },
    'blocked customerDelete',
  );
  const blockedRead = await run(customerReadDocument, { id: blockedCustomerId }, 'blocked read after customerDelete');
  const blockedCleanup = await bestEffortCleanup(blockedCustomerId, blockingOrderId);

  const noOrdersFixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    setup: {
      customerCreate: {
        operationName: 'CustomerDeleteOrderPreconditionCustomerCreate',
        query: customerCreateDocument,
        variables: controlCustomerVariables,
        response: controlCustomerCreate.payload,
      },
    },
    expected: {
      customerCreate: controlCustomerCreate.payload,
      customerDelete: controlDelete.payload,
      readAfterDelete: controlRead.payload,
    },
    cleanup: {},
    upstreamCalls: [],
  };

  const blockedFixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    setup: {
      customerCreate: {
        operationName: 'CustomerDeleteOrderPreconditionCustomerCreate',
        query: customerCreateDocument,
        variables: blockedCustomerVariables,
        response: blockedCustomerCreate.payload,
      },
      orderCreate: {
        operationName: 'CustomerDeleteOrderPreconditionOrderCreate',
        query: orderCreateDocument,
        variables: orderVariables,
        response: orderCreate.payload,
      },
    },
    expected: {
      customerCreate: blockedCustomerCreate.payload,
      orderCreate: orderCreate.payload,
      customerDelete: blockedDelete.payload,
      readAfterBlockedDelete: blockedRead.payload,
    },
    cleanup: blockedCleanup,
    upstreamCalls: [],
  };

  await Promise.all([
    writeFile(
      path.join(outputDir, 'customer-delete-no-orders-control.json'),
      `${JSON.stringify(noOrdersFixture, null, 2)}\n`,
      'utf8',
    ),
    writeFile(
      path.join(outputDir, 'customer-delete-blocked-by-orders.json'),
      `${JSON.stringify(blockedFixture, null, 2)}\n`,
      'utf8',
    ),
  ]);

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: ['customer-delete-no-orders-control.json', 'customer-delete-blocked-by-orders.json'],
        blockedCustomerId,
        blockingOrderId,
      },
      null,
      2,
    ),
  );
} catch (error) {
  const cleanup = await bestEffortCleanup(blockedCustomerId, blockingOrderId);
  console.error(JSON.stringify({ cleanup }, null, 2));
  throw error;
}
