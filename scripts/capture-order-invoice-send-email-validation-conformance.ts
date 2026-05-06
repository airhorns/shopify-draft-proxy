/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlPayload = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'orderInvoiceSend-email-validation.json');

const orderSelection = `
  id
  name
  email
  customer {
    id
    email
    displayName
  }
`;

const orderCreateMutation = `#graphql
  mutation OrderInvoiceSendEmailValidationCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ${orderSelection}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderReadQuery = `#graphql
  query OrderInvoiceSendEmailValidationRead($id: ID!) {
    order(id: $id) {
      ${orderSelection}
    }
  }
`;

const orderInvoiceSendMutation = `#graphql
  mutation OrderInvoiceSendEmailValidation($id: ID!, $email: EmailInput) {
    orderInvoiceSend(id: $id, email: $email) {
      order {
        ${orderSelection}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation OrderInvoiceSendEmailValidationCleanup($orderId: ID!, $reason: OrderCancelReason!, $notifyCustomer: Boolean!, $restock: Boolean!) {
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

function stripGraphqlTag(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function assertHttpOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function dataRoot(result: ConformanceGraphqlResult, root: string): GraphqlPayload {
  const data = result.payload.data;
  const value = typeof data === 'object' && data !== null ? (data as GraphqlPayload)[root] : null;
  if (typeof value !== 'object' || value === null) {
    throw new Error(`Missing ${root} payload: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return value as GraphqlPayload;
}

function orderFromRoot(result: ConformanceGraphqlResult, root: string): GraphqlPayload {
  const order = dataRoot(result, root).order;
  if (typeof order !== 'object' || order === null) {
    throw new Error(`Missing ${root}.order payload: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return order as GraphqlPayload;
}

function orderFromRead(result: ConformanceGraphqlResult): GraphqlPayload {
  const data = result.payload.data;
  const order = typeof data === 'object' && data !== null ? (data as GraphqlPayload).order : null;
  if (typeof order !== 'object' || order === null) {
    throw new Error(`Missing order read payload: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return order as GraphqlPayload;
}

function userErrors(result: ConformanceGraphqlResult): unknown[] {
  const value = dataRoot(result, 'orderInvoiceSend').userErrors;
  return Array.isArray(value) ? value : [];
}

function assertUserErrors(
  label: string,
  result: ConformanceGraphqlResult,
  expectedMessage: string,
  expectedField: unknown,
): void {
  const errors = userErrors(result);
  if (
    errors.length !== 1 ||
    typeof errors[0] !== 'object' ||
    errors[0] === null ||
    (errors[0] as GraphqlPayload).message !== expectedMessage ||
    (errors[0] as GraphqlPayload).code !== 'ORDER_INVOICE_SEND_UNSUCCESSFUL' ||
    JSON.stringify((errors[0] as GraphqlPayload).field ?? null) !== JSON.stringify(expectedField)
  ) {
    throw new Error(`${label} unexpected userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertNoUserErrors(label: string, result: ConformanceGraphqlResult): void {
  const errors = userErrors(result);
  if (errors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function orderCreateVariables(label: string, stamp: number, email?: string): Record<string, unknown> {
  const order: Record<string, unknown> = {
    note: `order invoice email validation ${label}`,
    tags: ['order-invoice-email-validation', label],
    test: true,
    currency: 'USD',
    lineItems: [
      {
        title: `Invoice validation ${label}`,
        quantity: 1,
        priceSet: {
          shopMoney: {
            amount: '1.00',
            currencyCode: 'USD',
          },
        },
        requiresShipping: false,
        taxable: false,
        sku: `invoice-validation-${label}-${stamp}`,
      },
    ],
  };
  if (email) {
    order.email = email;
  }
  return {
    order,
    options: {
      inventoryBehaviour: 'BYPASS',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  };
}

async function cleanupOrder(orderId: string): Promise<unknown> {
  const variables = {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  };
  const result = await runGraphqlRequest(orderCancelMutation, variables);
  return {
    query: stripGraphqlTag(orderCancelMutation),
    variables,
    response: result.payload,
  };
}

function hydrateCallFromOrder(order: GraphqlPayload, note: string): unknown {
  return {
    operationName: 'OrdersOrderHydrate',
    variables: {
      id: order.id,
    },
    query: note,
    response: {
      status: 200,
      body: {
        data: {
          order,
        },
      },
    },
  };
}

async function main(): Promise<void> {
  const stamp = Date.now();
  const cleanupOrderIds: string[] = [];
  const noEmailCreateVariables = orderCreateVariables('no-recipient', stamp);
  const happyEmail = `invoice-validation-${stamp}@example.com`;
  const happyCreateVariables = orderCreateVariables('order-email', stamp, happyEmail);

  try {
    const noEmailCreate = await runGraphqlRequest(orderCreateMutation, noEmailCreateVariables);
    assertHttpOk('no-recipient orderCreate', noEmailCreate);
    const noEmailOrder = orderFromRoot(noEmailCreate, 'orderCreate');
    const noEmailOrderId = String(noEmailOrder.id);
    cleanupOrderIds.push(noEmailOrderId);

    const happyCreate = await runGraphqlRequest(orderCreateMutation, happyCreateVariables);
    assertHttpOk('order-email orderCreate', happyCreate);
    const happyOrder = orderFromRoot(happyCreate, 'orderCreate');
    const happyOrderId = String(happyOrder.id);
    cleanupOrderIds.push(happyOrderId);

    const noEmailRead = await runGraphqlRequest(orderReadQuery, { id: noEmailOrderId });
    assertHttpOk('no-recipient order read', noEmailRead);
    const noEmailHydrateOrder = orderFromRead(noEmailRead);

    const happyRead = await runGraphqlRequest(orderReadQuery, { id: happyOrderId });
    assertHttpOk('order-email order read', happyRead);
    const happyHydrateOrder = orderFromRead(happyRead);

    const noEmailVariables = { id: noEmailOrderId };
    const noEmailResult = await runGraphqlRequest(orderInvoiceSendMutation, noEmailVariables);
    assertHttpOk('no-recipient orderInvoiceSend', noEmailResult);
    assertUserErrors('no-recipient orderInvoiceSend', noEmailResult, 'No recipient email address was provided', null);

    const invalidVariables = {
      id: noEmailOrderId,
      email: {
        to: 'not an email',
      },
    };
    const invalidResult = await runGraphqlRequest(orderInvoiceSendMutation, invalidVariables);
    assertHttpOk('invalid-email orderInvoiceSend', invalidResult);
    assertUserErrors('invalid-email orderInvoiceSend', invalidResult, 'To is invalid', null);

    const happyVariables = { id: happyOrderId };
    const happyResult = await runGraphqlRequest(orderInvoiceSendMutation, happyVariables);
    assertHttpOk('order-email orderInvoiceSend', happyResult);
    assertNoUserErrors('order-email orderInvoiceSend', happyResult);

    const cleanup = {
      noRecipientOrder: await cleanupOrder(noEmailOrderId),
      orderEmailOrder: await cleanupOrder(happyOrderId),
    };
    cleanupOrderIds.length = 0;

    await writeJson(fixturePath, {
      cases: [
        {
          name: 'no-recipient-email-required',
          variables: noEmailVariables,
          setup: {
            orderCreate: {
              query: stripGraphqlTag(orderCreateMutation),
              variables: noEmailCreateVariables,
              response: noEmailCreate.payload,
            },
            orderRead: {
              query: stripGraphqlTag(orderReadQuery),
              variables: { id: noEmailOrderId },
              response: noEmailRead.payload,
            },
          },
          mutation: {
            query: stripGraphqlTag(orderInvoiceSendMutation),
            response: noEmailResult.payload,
          },
        },
        {
          name: 'malformed-explicit-email',
          variables: invalidVariables,
          mutation: {
            query: stripGraphqlTag(orderInvoiceSendMutation),
            response: invalidResult.payload,
          },
        },
        {
          name: 'order-email-happy-path',
          variables: happyVariables,
          setup: {
            orderCreate: {
              query: stripGraphqlTag(orderCreateMutation),
              variables: happyCreateVariables,
              response: happyCreate.payload,
            },
            orderRead: {
              query: stripGraphqlTag(orderReadQuery),
              variables: { id: happyOrderId },
              response: happyRead.payload,
            },
          },
          mutation: {
            query: stripGraphqlTag(orderInvoiceSendMutation),
            response: happyResult.payload,
          },
        },
      ],
      cleanup,
      upstreamCalls: [
        hydrateCallFromOrder(
          noEmailHydrateOrder,
          'hand-synthesized from orderInvoiceSend email validation no-recipient setup read for Pattern 2 order hydration',
        ),
        hydrateCallFromOrder(
          happyHydrateOrder,
          'hand-synthesized from orderInvoiceSend email validation happy-path setup read for Pattern 2 order hydration',
        ),
      ],
    });

    console.log(`Wrote ${fixturePath}`);
  } finally {
    for (const orderId of cleanupOrderIds) {
      await cleanupOrder(orderId);
    }
  }
}

await main();
