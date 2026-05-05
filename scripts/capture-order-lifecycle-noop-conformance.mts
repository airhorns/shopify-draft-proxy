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
const closeFixturePath = path.join(fixtureDir, 'orderClose-noop-on-already-closed.json');
const openFixturePath = path.join(fixtureDir, 'orderOpen-noop-on-already-open.json');

const orderSelection = `
  id
  closed
  closedAt
  updatedAt
`;

const orderCreateMutation = `#graphql
  mutation OrderLifecycleNoopCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
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
  query OrderLifecycleNoopRead($id: ID!) {
    order(id: $id) {
      ${orderSelection}
    }
  }
`;

const orderCloseMutation = `#graphql
  mutation OrderCloseNoopOnAlreadyClosed($input: OrderCloseInput!) {
    orderClose(input: $input) {
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

const orderOpenMutation = `#graphql
  mutation OrderOpenNoopOnAlreadyOpen($input: OrderOpenInput!) {
    orderOpen(input: $input) {
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

const orderCancelMutation = `#graphql
  mutation OrderLifecycleNoopCleanup($orderId: ID!, $reason: OrderCancelReason!, $notifyCustomer: Boolean!, $restock: Boolean!) {
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
  const value = typeof data === 'object' && data !== null ? (data as Record<string, unknown>)[root] : null;
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
  const order = typeof data === 'object' && data !== null ? (data as Record<string, unknown>).order : null;
  if (typeof order !== 'object' || order === null) {
    throw new Error(`Missing order read payload: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return order as GraphqlPayload;
}

function assertEmptyUserErrors(label: string, result: ConformanceGraphqlResult, root: string): void {
  const userErrors = dataRoot(result, root).userErrors;
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertSameLifecycleFields(label: string, left: GraphqlPayload, right: GraphqlPayload): void {
  for (const field of ['id', 'closed', 'closedAt', 'updatedAt']) {
    if (left[field] !== right[field]) {
      throw new Error(
        `${label} changed ${field}: ${JSON.stringify({ before: left[field], after: right[field] }, null, 2)}`,
      );
    }
  }
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

function orderCreateVariables(label: string, stamp: number): Record<string, unknown> {
  return {
    order: {
      email: `har-588-${label}-${stamp}@example.com`,
      note: `HAR-588 order lifecycle no-op ${label}`,
      tags: ['har-588', 'order-lifecycle-noop', label],
      test: true,
      currency: 'USD',
      lineItems: [
        {
          title: `HAR-588 ${label} custom item`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '1.00',
              currencyCode: 'USD',
            },
          },
          requiresShipping: false,
          taxable: false,
          sku: `har-588-${label}-${stamp}`,
        },
      ],
      transactions: [
        {
          kind: 'SALE',
          status: 'SUCCESS',
          gateway: 'manual',
          test: true,
          amountSet: {
            shopMoney: {
              amount: '1.00',
              currencyCode: 'USD',
            },
          },
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

async function captureCloseNoop(stamp: number): Promise<void> {
  const createVariables = orderCreateVariables('already-closed', stamp);
  const createResult = await runGraphqlRequest(orderCreateMutation, createVariables);
  assertHttpOk('close no-op orderCreate', createResult);
  assertEmptyUserErrors('close no-op orderCreate', createResult, 'orderCreate');
  const createdOrder = orderFromRoot(createResult, 'orderCreate');
  const id = String(createdOrder.id);
  const variables = { input: { id } };

  const firstCloseResult = await runGraphqlRequest(orderCloseMutation, variables);
  assertHttpOk('setup orderClose', firstCloseResult);
  assertEmptyUserErrors('setup orderClose', firstCloseResult, 'orderClose');
  const beforeReadResult = await runGraphqlRequest(orderReadQuery, { id });
  assertHttpOk('before redundant orderClose read', beforeReadResult);
  const beforeOrder = orderFromRead(beforeReadResult);

  await delay(1500);

  const noOpResult = await runGraphqlRequest(orderCloseMutation, variables);
  assertHttpOk('redundant orderClose', noOpResult);
  assertEmptyUserErrors('redundant orderClose', noOpResult, 'orderClose');
  assertSameLifecycleFields('redundant orderClose response', beforeOrder, orderFromRoot(noOpResult, 'orderClose'));
  const afterReadResult = await runGraphqlRequest(orderReadQuery, { id });
  assertHttpOk('after redundant orderClose read', afterReadResult);
  assertSameLifecycleFields('redundant orderClose downstream read', beforeOrder, orderFromRead(afterReadResult));

  const cleanupOpenResult = await runGraphqlRequest(orderOpenMutation, variables);
  const cleanupCancel = await cleanupOrder(id);

  await writeJson(closeFixturePath, {
    variables,
    setup: {
      orderCreate: {
        query: stripGraphqlTag(orderCreateMutation),
        variables: createVariables,
        response: createResult.payload,
      },
      firstClose: {
        query: stripGraphqlTag(orderCloseMutation),
        variables,
        response: firstCloseResult.payload,
      },
      beforeNoopRead: {
        query: stripGraphqlTag(orderReadQuery),
        variables: { id },
        response: beforeReadResult.payload,
      },
    },
    mutation: {
      query: stripGraphqlTag(orderCloseMutation),
      response: noOpResult.payload,
    },
    downstreamRead: {
      variables: { id },
      response: afterReadResult.payload,
    },
    cleanup: {
      open: {
        query: stripGraphqlTag(orderOpenMutation),
        variables,
        response: cleanupOpenResult.payload,
      },
      cancel: cleanupCancel,
    },
    upstreamCalls: [
      hydrateCallFromOrder(
        beforeOrder,
        'hand-synthesized from HAR-588 beforeNoopRead for redundant orderClose Pattern 2 order hydration',
      ),
    ],
  });
}

async function captureOpenNoop(stamp: number): Promise<void> {
  const createVariables = orderCreateVariables('already-open', stamp);
  const createResult = await runGraphqlRequest(orderCreateMutation, createVariables);
  assertHttpOk('open no-op orderCreate', createResult);
  assertEmptyUserErrors('open no-op orderCreate', createResult, 'orderCreate');
  const createdOrder = orderFromRoot(createResult, 'orderCreate');
  const id = String(createdOrder.id);
  const variables = { input: { id } };
  const beforeReadResult = await runGraphqlRequest(orderReadQuery, { id });
  assertHttpOk('before redundant orderOpen read', beforeReadResult);
  const beforeOrder = orderFromRead(beforeReadResult);

  await delay(1500);

  const noOpResult = await runGraphqlRequest(orderOpenMutation, variables);
  assertHttpOk('redundant orderOpen', noOpResult);
  assertEmptyUserErrors('redundant orderOpen', noOpResult, 'orderOpen');
  assertSameLifecycleFields('redundant orderOpen response', beforeOrder, orderFromRoot(noOpResult, 'orderOpen'));
  const afterReadResult = await runGraphqlRequest(orderReadQuery, { id });
  assertHttpOk('after redundant orderOpen read', afterReadResult);
  assertSameLifecycleFields('redundant orderOpen downstream read', beforeOrder, orderFromRead(afterReadResult));

  const cleanupCancel = await cleanupOrder(id);

  await writeJson(openFixturePath, {
    variables,
    setup: {
      orderCreate: {
        query: stripGraphqlTag(orderCreateMutation),
        variables: createVariables,
        response: createResult.payload,
      },
      beforeNoopRead: {
        query: stripGraphqlTag(orderReadQuery),
        variables: { id },
        response: beforeReadResult.payload,
      },
    },
    mutation: {
      query: stripGraphqlTag(orderOpenMutation),
      response: noOpResult.payload,
    },
    downstreamRead: {
      variables: { id },
      response: afterReadResult.payload,
    },
    cleanup: {
      cancel: cleanupCancel,
    },
    upstreamCalls: [
      hydrateCallFromOrder(
        beforeOrder,
        'hand-synthesized from HAR-588 beforeNoopRead for redundant orderOpen Pattern 2 order hydration',
      ),
    ],
  });
}

const stamp = Date.now();
await captureCloseNoop(stamp);
await captureOpenNoop(stamp);

console.log(`Wrote ${closeFixturePath}`);
console.log(`Wrote ${openFixturePath}`);
