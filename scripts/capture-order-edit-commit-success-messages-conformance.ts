/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import { createConformanceCapture, readRecord, requireString, type JsonRecord } from './conformance-capture-lib.js';

const cap = await createConformanceCapture();

const scenarioId = 'orderEditCommit-success-messages';
const fixturePath = cap.fixturePath('orders', 'order-edit-commit-success-messages.json');
const paritySpecPath = 'config/parity-specs/orders/orderEditCommit-success-messages.json';

const createDocument = await cap.readRequest('orders', 'orderEditCommit-success-messages-create.graphql');
const beginDocument = await cap.readRequest('orders', 'orderEditCommit-success-messages-begin.graphql');
const closeDocument = await cap.readRequest('orders', 'orderEditCommit-success-messages-close.graphql');
const setQuantityDocument = await cap.readRequest('orders', 'orderEditCommit-success-messages-setQuantity.graphql');
const commitDocument = await cap.readRequest('orders', 'orderEditCommit-success-messages-commit.graphql');

const cancelMutation = `#graphql
  mutation OrderEditCommitSuccessMessagesCancel(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
    $refund: Boolean!
  ) {
    orderCancel(
      orderId: $orderId
      reason: $reason
      notifyCustomer: $notifyCustomer
      restock: $restock
      refund: $refund
    ) {
      job { id }
      userErrors { field message }
    }
  }
`;

type CaptureStep = {
  query: string;
  variables: JsonRecord;
  response: {
    status: number;
    payload: JsonRecord;
  };
};

type CaseCapture = {
  create: CaptureStep;
  close?: CaptureStep;
  begin: CaptureStep;
  setQuantity: CaptureStep;
  commit: CaptureStep;
};

type SuccessMessageCases = {
  notifyFalse: CaseCapture;
  paidNotify: CaseCapture;
  balanceNotify: CaseCapture;
  unarchiveNotify: CaseCapture;
};

const createdOrderIds: string[] = [];
const cleanup: JsonRecord[] = [];

function moneySet(amount: string, currencyCode: string): JsonRecord {
  return {
    shopMoney: { amount, currencyCode },
    presentmentMoney: { amount, currencyCode },
  };
}

function saleTransaction(amount: string, currencyCode: string): JsonRecord {
  return {
    kind: 'SALE',
    status: 'SUCCESS',
    gateway: 'manual',
    test: true,
    amountSet: moneySet(amount, currencyCode),
  };
}

function multiplyAmount(amount: string, multiplier: number): string {
  return (Number.parseFloat(amount) * multiplier).toFixed(2);
}

function orderInput(label: string, amount: string, paid: boolean, currencyCode: string): JsonRecord {
  const order: JsonRecord = {
    email: `order-edit-commit-success-${label}-${cap.stamp}@example.com`,
    note: `order edit commit success messages ${label}`,
    tags: ['order-edit-commit-success-messages', label],
    test: true,
    currency: currencyCode,
    presentmentCurrency: currencyCode,
    financialStatus: paid ? 'PAID' : 'PENDING',
    lineItems: [
      {
        title: `Order edit commit success ${label}`,
        quantity: 2,
        priceSet: moneySet(amount, currencyCode),
        requiresShipping: false,
        taxable: false,
        sku: `order-edit-commit-success-${label}-${cap.stamp}`,
      },
    ],
  };
  if (paid) {
    order['transactions'] = [saleTransaction(multiplyAmount(amount, 2), currencyCode)];
  }
  return order;
}

async function captureStep(query: string, variables: JsonRecord, label: string): Promise<CaptureStep> {
  return {
    query,
    variables,
    response: {
      status: 200,
      payload: await cap.run(query, variables, label),
    },
  };
}

function mutationRoot(step: CaptureStep, rootName: string, label: string): JsonRecord {
  return cap.mutationRoot(step.response.payload, rootName, label);
}

function orderIdFromCreate(step: CaptureStep, label: string): string {
  const order = readRecord(mutationRoot(step, 'orderCreate', label)['order']);
  return requireString(order?.['id'], `${label} order id`);
}

function calculatedOrderIdFromBegin(step: CaptureStep, label: string): string {
  const calculatedOrder = readRecord(mutationRoot(step, 'orderEditBegin', label)['calculatedOrder']);
  return requireString(calculatedOrder?.['id'], `${label} calculated order id`);
}

function calculatedLineItemIdFromBegin(step: CaptureStep, label: string): string {
  const calculatedOrder = readRecord(mutationRoot(step, 'orderEditBegin', label)['calculatedOrder']);
  const lineItems = readRecord(calculatedOrder?.['lineItems']);
  const nodes = lineItems?.['nodes'];
  const firstLineItem = Array.isArray(nodes) ? readRecord(nodes[0]) : null;
  return requireString(firstLineItem?.['id'], `${label} calculated line item id`);
}

async function captureCase(options: {
  label: string;
  amount: string;
  paid: boolean;
  notifyCustomer: boolean;
  closeBeforeBegin?: boolean;
  currencyCode: string;
}): Promise<CaseCapture> {
  const create = await captureStep(
    createDocument,
    {
      order: orderInput(options.label, options.amount, options.paid, options.currencyCode),
      options: { inventoryBehaviour: 'BYPASS', sendReceipt: false, sendFulfillmentReceipt: false },
    },
    `${options.label} create`,
  );
  const orderId = orderIdFromCreate(create, `${options.label} create`);
  createdOrderIds.push(orderId);

  let close: CaptureStep | undefined;
  if (options.closeBeforeBegin) {
    close = await captureStep(closeDocument, { input: { id: orderId } }, `${options.label} close before edit`);
    mutationRoot(close, 'orderClose', `${options.label} close before edit`);
  }

  const begin = await captureStep(beginDocument, { id: orderId }, `${options.label} begin`);
  const calculatedOrderId = calculatedOrderIdFromBegin(begin, `${options.label} begin`);
  const calculatedLineItemId = calculatedLineItemIdFromBegin(begin, `${options.label} begin`);
  const setQuantity = await captureStep(
    setQuantityDocument,
    { id: calculatedOrderId, lineItemId: calculatedLineItemId, quantity: 1 },
    `${options.label} set quantity`,
  );
  mutationRoot(setQuantity, 'orderEditSetQuantity', `${options.label} set quantity`);
  const commit = await captureStep(
    commitDocument,
    {
      id: calculatedOrderId,
      notifyCustomer: options.notifyCustomer,
      staffNote: `order edit commit success messages ${options.label}`,
    },
    `${options.label} commit`,
  );
  mutationRoot(commit, 'orderEditCommit', `${options.label} commit`);

  return close ? { create, close, begin, setQuantity, commit } : { create, begin, setQuantity, commit };
}

async function cleanupOrders(): Promise<void> {
  const uniqueOrderIds = [...new Set(createdOrderIds)].reverse();
  for (const orderId of uniqueOrderIds) {
    try {
      cleanup.push({
        orderId,
        result: await cap.runGraphqlRequest(cancelMutation, {
          orderId,
          reason: 'OTHER',
          notifyCustomer: false,
          restock: false,
          refund: false,
        }),
      });
    } catch (error) {
      cleanup.push({ orderId, error: (error as Error).message });
    }
  }
}

async function captureCases(): Promise<SuccessMessageCases> {
  const discovery = await cap.run(
    `#graphql
      query OrderEditCommitSuccessMessagesDiscover {
        shop { currencyCode }
      }
    `,
    {},
    'discover shop currency',
  );
  const currencyCode = requireString(readRecord(readRecord(discovery['data'])?.['shop'])?.['currencyCode'], 'currency');

  try {
    return {
      notifyFalse: await captureCase({
        label: 'notify-false',
        amount: '10.00',
        paid: true,
        notifyCustomer: false,
        currencyCode,
      }),
      paidNotify: await captureCase({
        label: 'paid-notify',
        amount: '12.00',
        paid: true,
        notifyCustomer: true,
        currencyCode,
      }),
      balanceNotify: await captureCase({
        label: 'balance-notify',
        amount: '14.00',
        paid: false,
        notifyCustomer: true,
        currencyCode,
      }),
      unarchiveNotify: await captureCase({
        label: 'unarchive-notify',
        amount: '16.00',
        paid: true,
        notifyCustomer: true,
        closeBeforeBegin: true,
        currencyCode,
      }),
    };
  } finally {
    await cleanupOrders();
  }
}

const cases = await captureCases();

await cap.writeJson(fixturePath, {
  scenarioId,
  apiVersion: cap.apiVersion,
  storeDomain: cap.storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  notes:
    'Live Shopify orderEditCommit successMessages capture covering notifyCustomer false, notifyCustomer true on a paid order, notifyCustomer true on a balance-due order, and commit after closing an order. The proxy parity replay creates and edits local orders through public GraphQL requests only; no private state setup is used.',
  cases,
  cleanup,
  upstreamCalls: [],
});

await cap.writeJson(paritySpecPath, {
  scenarioId,
  operationNames: ['orderCreate', 'orderClose', 'orderEditBegin', 'orderEditSetQuantity', 'orderEditCommit'],
  scenarioStatus: 'captured',
  assertionKinds: ['payload-shape', 'selected-fields'],
  liveCaptureFiles: [fixturePath],
  proxyRequest: {
    documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-create.graphql',
    variablesCapturePath: '$.cases.notifyFalse.create.variables',
    apiVersion: cap.apiVersion,
  },
  comparisonMode: 'captured-vs-proxy-request',
  notes:
    'Captured Shopify 2026-04 successMessages branches for orderEditCommit: default update message when notifyCustomer is false, Notification sent for a paid order, Invoice sent for a balance-due order, and Order unarchived before the notify message when committing an edit reopens a closed order.',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [],
    targets: [
      {
        name: 'notify-false-create-user-errors',
        capturePath: '$.cases.notifyFalse.create.response.payload.data.orderCreate.userErrors',
        proxyPath: '$.data.orderCreate.userErrors',
      },
      {
        name: 'notify-false-begin-user-errors',
        capturePath: '$.cases.notifyFalse.begin.response.payload.data.orderEditBegin.userErrors',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-begin.graphql',
          variables: {
            id: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditBegin.userErrors',
      },
      {
        name: 'notify-false-set-quantity-user-errors',
        capturePath: '$.cases.notifyFalse.setQuantity.response.payload.data.orderEditSetQuantity.userErrors',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-setQuantity.graphql',
          variables: {
            id: { fromPreviousProxyPath: '$.data.orderEditBegin.calculatedOrder.id' },
            lineItemId: {
              fromPreviousProxyPath: '$.data.orderEditBegin.calculatedOrder.lineItems.nodes[0].id',
            },
            quantity: 1,
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditSetQuantity.userErrors',
      },
      {
        name: 'notify-false-success-messages',
        capturePath: '$.cases.notifyFalse.commit.response.payload.data.orderEditCommit.successMessages',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-commit.graphql',
          variables: {
            id: { fromPreviousProxyPath: '$.data.orderEditSetQuantity.calculatedOrder.id' },
            notifyCustomer: false,
            staffNote: 'order edit commit success messages notify-false',
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditCommit.successMessages',
      },
      {
        name: 'paid-notify-create-user-errors',
        capturePath: '$.cases.paidNotify.create.response.payload.data.orderCreate.userErrors',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-create.graphql',
          variablesCapturePath: '$.cases.paidNotify.create.variables',
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderCreate.userErrors',
      },
      {
        name: 'paid-notify-begin-user-errors',
        capturePath: '$.cases.paidNotify.begin.response.payload.data.orderEditBegin.userErrors',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-begin.graphql',
          variables: {
            id: { fromPreviousProxyPath: '$.data.orderCreate.order.id' },
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditBegin.userErrors',
      },
      {
        name: 'paid-notify-set-quantity-user-errors',
        capturePath: '$.cases.paidNotify.setQuantity.response.payload.data.orderEditSetQuantity.userErrors',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-setQuantity.graphql',
          variables: {
            id: { fromPreviousProxyPath: '$.data.orderEditBegin.calculatedOrder.id' },
            lineItemId: {
              fromPreviousProxyPath: '$.data.orderEditBegin.calculatedOrder.lineItems.nodes[0].id',
            },
            quantity: 1,
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditSetQuantity.userErrors',
      },
      {
        name: 'paid-notify-success-messages',
        capturePath: '$.cases.paidNotify.commit.response.payload.data.orderEditCommit.successMessages',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-commit.graphql',
          variables: {
            id: { fromPreviousProxyPath: '$.data.orderEditSetQuantity.calculatedOrder.id' },
            notifyCustomer: true,
            staffNote: 'order edit commit success messages paid-notify',
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditCommit.successMessages',
      },
      {
        name: 'balance-notify-create-user-errors',
        capturePath: '$.cases.balanceNotify.create.response.payload.data.orderCreate.userErrors',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-create.graphql',
          variablesCapturePath: '$.cases.balanceNotify.create.variables',
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderCreate.userErrors',
      },
      {
        name: 'balance-notify-begin-user-errors',
        capturePath: '$.cases.balanceNotify.begin.response.payload.data.orderEditBegin.userErrors',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-begin.graphql',
          variables: {
            id: { fromPreviousProxyPath: '$.data.orderCreate.order.id' },
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditBegin.userErrors',
      },
      {
        name: 'balance-notify-set-quantity-user-errors',
        capturePath: '$.cases.balanceNotify.setQuantity.response.payload.data.orderEditSetQuantity.userErrors',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-setQuantity.graphql',
          variables: {
            id: { fromPreviousProxyPath: '$.data.orderEditBegin.calculatedOrder.id' },
            lineItemId: {
              fromPreviousProxyPath: '$.data.orderEditBegin.calculatedOrder.lineItems.nodes[0].id',
            },
            quantity: 1,
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditSetQuantity.userErrors',
      },
      {
        name: 'balance-notify-success-messages',
        capturePath: '$.cases.balanceNotify.commit.response.payload.data.orderEditCommit.successMessages',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-commit.graphql',
          variables: {
            id: { fromPreviousProxyPath: '$.data.orderEditSetQuantity.calculatedOrder.id' },
            notifyCustomer: true,
            staffNote: 'order edit commit success messages balance-notify',
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditCommit.successMessages',
      },
      {
        name: 'unarchive-notify-create-user-errors',
        capturePath: '$.cases.unarchiveNotify.create.response.payload.data.orderCreate.userErrors',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-create.graphql',
          variablesCapturePath: '$.cases.unarchiveNotify.create.variables',
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderCreate.userErrors',
      },
      {
        name: 'unarchive-notify-close-state',
        capturePath: '$.cases.unarchiveNotify.close.response.payload.data.orderClose.order',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-close.graphql',
          variables: {
            input: {
              id: { fromPreviousProxyPath: '$.data.orderCreate.order.id' },
            },
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderClose.order',
        selectedPaths: ['$.closed'],
      },
      {
        name: 'unarchive-notify-begin-user-errors',
        capturePath: '$.cases.unarchiveNotify.begin.response.payload.data.orderEditBegin.userErrors',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-begin.graphql',
          variables: {
            id: { fromPreviousProxyPath: '$.data.orderClose.order.id' },
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditBegin.userErrors',
      },
      {
        name: 'unarchive-notify-set-quantity-user-errors',
        capturePath: '$.cases.unarchiveNotify.setQuantity.response.payload.data.orderEditSetQuantity.userErrors',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-setQuantity.graphql',
          variables: {
            id: { fromPreviousProxyPath: '$.data.orderEditBegin.calculatedOrder.id' },
            lineItemId: {
              fromPreviousProxyPath: '$.data.orderEditBegin.calculatedOrder.lineItems.nodes[0].id',
            },
            quantity: 1,
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditSetQuantity.userErrors',
      },
      {
        name: 'unarchive-notify-success-messages-and-open-state',
        capturePath: '$.cases.unarchiveNotify.commit.response.payload.data.orderEditCommit',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-success-messages-commit.graphql',
          variables: {
            id: { fromPreviousProxyPath: '$.data.orderEditSetQuantity.calculatedOrder.id' },
            notifyCustomer: true,
            staffNote: 'order edit commit success messages unarchive-notify',
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditCommit',
        selectedPaths: ['$.successMessages', '$.order.closed', '$.order.closedAt'],
      },
    ],
  },
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      paritySpecPath,
      apiVersion: cap.apiVersion,
      storeDomain: cap.storeDomain,
      successMessages: {
        notifyFalse: readRecord(cases.notifyFalse.commit.response.payload['data'])?.['orderEditCommit'],
        paidNotify: readRecord(cases.paidNotify.commit.response.payload['data'])?.['orderEditCommit'],
        balanceNotify: readRecord(cases.balanceNotify.commit.response.payload['data'])?.['orderEditCommit'],
        unarchiveNotify: readRecord(cases.unarchiveNotify.commit.response.payload['data'])?.['orderEditCommit'],
      },
      cleanupCount: cleanup.length,
    },
    null,
    2,
  ),
);
