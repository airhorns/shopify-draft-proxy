/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import { createConformanceCapture, readRecord, requireString, type JsonRecord } from './conformance-capture-lib.js';
import { captureDraftProxyShopPricingHydrate } from './support/shopify/runtime-hydration-capture.js';

const cap = await createConformanceCapture();
const shopPricingHydrate = await captureDraftProxyShopPricingHydrate((query, variables) =>
  cap.runGraphqlRequest(query, variables),
);

const draftCreateDocument = await cap.readRequest('orders', 'orderEditCommit-derived-statuses-draftCreate.graphql');
const draftCompleteDocument = await cap.readRequest('orders', 'orderEditCommit-derived-statuses-draftComplete.graphql');
const beginDocument = await cap.readRequest('orders', 'orderEditCommit-derived-statuses-begin.graphql');
const addCustomItemDocument = await cap.readRequest('orders', 'orderEditCommit-derived-statuses-addCustomItem.graphql');
const commitDocument = await cap.readRequest('orders', 'orderEditCommit-derived-statuses-commit.graphql');
const downstreamReadDocument = await cap.readRequest('orders', 'orderEditCommit-derived-statuses-read.graphql');

const cancelMutation = `#graphql
  mutation OrderEditCommitDerivedStatusesCancel(
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
      job {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

function captureRecord(query: string, variables: JsonRecord, response: JsonRecord): JsonRecord {
  return { query: query.trim(), variables, response };
}

function mutationPayload(payload: JsonRecord, rootName: string): JsonRecord {
  return cap.mutationRoot(payload, rootName, rootName);
}

function draftOrderId(payload: JsonRecord): string {
  const root = mutationPayload(payload, 'draftOrderCreate');
  return requireString(readRecord(root['draftOrder'])?.['id'], 'draft order id');
}

function completedOrderId(payload: JsonRecord): string {
  const root = mutationPayload(payload, 'draftOrderComplete');
  const draftOrder = readRecord(root['draftOrder']);
  return requireString(readRecord(draftOrder?.['order'])?.['id'], 'completed order id');
}

function calculatedOrderId(payload: JsonRecord): string {
  const root = mutationPayload(payload, 'orderEditBegin');
  return requireString(readRecord(root['calculatedOrder'])?.['id'], 'calculated order id');
}

const currencyCode = 'CAD';

const draftCreateVariables = {
  input: {
    email: `order-edit-derived-statuses-${cap.stamp}@example.com`,
    note: `order edit derived statuses capture ${cap.stamp}`,
    tags: ['order-edit-derived-statuses', cap.stamp],
    lineItems: [
      {
        title: `Original paid editable line ${cap.stamp}`,
        quantity: 1,
        originalUnitPriceWithCurrency: {
          amount: '10.00',
          currencyCode,
        },
        requiresShipping: true,
        taxable: false,
      },
    ],
    shippingAddress: {
      firstName: 'Conformance',
      lastName: 'OrderEdit',
      address1: '123 Queen St W',
      city: 'Toronto',
      provinceCode: 'ON',
      countryCode: 'CA',
      zip: 'M5H 2M9',
    },
  },
};

const draftCreate = await cap.run(draftCreateDocument, draftCreateVariables, 'create draft order');
const draftId = draftOrderId(draftCreate);

const draftCompleteVariables = {
  id: draftId,
  paymentPending: false,
};
const draftComplete = await cap.run(draftCompleteDocument, draftCompleteVariables, 'complete paid draft order');
const orderId = completedOrderId(draftComplete);

const beginVariables = { id: orderId };
const begin = await cap.run(beginDocument, beginVariables, 'begin edit');
const calcId = calculatedOrderId(begin);

const addCustomItemVariables = {
  id: calcId,
  title: 'Added unpaid item',
  quantity: 1,
  price: {
    amount: '5.00',
    currencyCode,
  },
};
const addCustomItem = await cap.run(addCustomItemDocument, addCustomItemVariables, 'add custom item');
mutationPayload(addCustomItem, 'orderEditAddCustomItem');

const commitVariables = {
  id: calcId,
  notifyCustomer: false,
  staffNote: 'order edit derived statuses capture',
};
const commit = await cap.run(commitDocument, commitVariables, 'commit edit');
mutationPayload(commit, 'orderEditCommit');

const downstreamReadVariables = { id: orderId };
const downstreamRead = await cap.run(downstreamReadDocument, downstreamReadVariables, 'downstream order read');
if (!readRecord(readRecord(downstreamRead['data'])?.['order'])) {
  throw new Error(`Expected downstream order: ${JSON.stringify(downstreamRead, null, 2)}`);
}

const cleanup = await cap.runGraphqlRequest(cancelMutation, {
  orderId,
  reason: 'OTHER',
  notifyCustomer: false,
  restock: true,
  refund: false,
});

const fixturePath = cap.fixturePath('orders', 'order-edit-commit-derived-statuses.json');
const specPath = 'config/parity-specs/orders/orderEditCommit-derived-statuses.json';

await cap.writeJson(fixturePath, {
  scenarioId: 'order-edit-commit-derived-statuses',
  apiVersion: cap.apiVersion,
  storeDomain: cap.storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  notes:
    'Live draftOrderCreate -> draftOrderComplete -> orderEditBegin -> orderEditAddCustomItem -> orderEditCommit capture proving a paid editable order becomes partially paid after adding an unpaid custom item; post-edit display statuses, totalOutstandingSet, totalPriceSet, and currentTotalPriceSet are asserted from the commit payload and downstream order read.',
  draftCreate: captureRecord(draftCreateDocument, draftCreateVariables, draftCreate),
  draftComplete: captureRecord(draftCompleteDocument, draftCompleteVariables, draftComplete),
  begin: captureRecord(beginDocument, beginVariables, begin),
  addCustomItem: captureRecord(addCustomItemDocument, addCustomItemVariables, addCustomItem),
  commit: captureRecord(commitDocument, commitVariables, commit),
  downstreamRead: captureRecord(downstreamReadDocument, downstreamReadVariables, downstreamRead),
  cleanup: {
    query: cancelMutation.trim(),
    variables: {
      orderId,
      reason: 'OTHER',
      notifyCustomer: false,
      restock: true,
      refund: false,
    },
    status: cleanup.status,
    response: cleanup.payload,
  },
  upstreamCalls: [shopPricingHydrate],
});

await cap.writeJson(specPath, {
  scenarioId: 'order-edit-commit-derived-statuses',
  operationNames: [
    'draftOrderCreate',
    'draftOrderComplete',
    'orderEditBegin',
    'orderEditAddCustomItem',
    'orderEditCommit',
    'order',
  ],
  scenarioStatus: 'captured',
  assertionKinds: ['downstream-read-parity', 'derived-status-parity', 'money-totals-parity'],
  liveCaptureFiles: [fixturePath],
  proxyRequest: {
    documentPath: 'config/parity-requests/orders/orderEditCommit-derived-statuses-draftCreate.graphql',
    variablesCapturePath: '$.draftCreate.variables',
    apiVersion: cap.apiVersion,
  },
  comparisonMode: 'captured-vs-proxy-request',
  notes:
    'Live captured draft-order setup followed by orderEditBegin -> orderEditAddCustomItem -> orderEditCommit. The strict targets assert that the commit payload and downstream order read rederive displayFinancialStatus, displayFulfillmentStatus, totalOutstandingSet, totalPriceSet, and currentTotalPriceSet after the added unpaid item. Volatile draft, order, calculated-order, and calculated-line IDs are excluded.',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [],
    targets: [
      {
        name: 'draft-create-payload',
        capturePath: '$.draftCreate.response.data.draftOrderCreate',
        proxyPath: '$.data.draftOrderCreate',
        excludedPaths: ['$.draftOrder.id'],
      },
      {
        name: 'draft-complete-paid-order-state',
        capturePath: '$.draftComplete.response.data.draftOrderComplete.draftOrder.order',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-derived-statuses-draftComplete.graphql',
          variables: {
            id: {
              fromProxyResponse: 'draft-create-payload',
              path: '$.data.draftOrderCreate.draftOrder.id',
            },
            paymentPending: false,
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.draftOrderComplete.draftOrder.order',
        excludedPaths: ['$.id'],
      },
      {
        name: 'begin-edit-payload',
        capturePath: '$.begin.response.data.orderEditBegin',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-derived-statuses-begin.graphql',
          variables: {
            id: {
              fromProxyResponse: 'draft-complete-paid-order-state',
              path: '$.data.draftOrderComplete.draftOrder.order.id',
            },
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditBegin',
        excludedPaths: ['$.calculatedOrder.id', '$.calculatedOrder.originalOrder.id'],
      },
      {
        name: 'add-custom-item-payload',
        capturePath: '$.addCustomItem.response.data.orderEditAddCustomItem',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-derived-statuses-addCustomItem.graphql',
          variables: {
            id: {
              fromProxyResponse: 'begin-edit-payload',
              path: '$.data.orderEditBegin.calculatedOrder.id',
            },
            title: {
              fromCapturePath: '$.addCustomItem.variables.title',
            },
            quantity: {
              fromCapturePath: '$.addCustomItem.variables.quantity',
            },
            price: {
              fromCapturePath: '$.addCustomItem.variables.price',
            },
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditAddCustomItem',
        excludedPaths: [
          '$.calculatedOrder.id',
          '$.calculatedOrder.addedLineItems.nodes[*].id',
          '$.calculatedLineItem.id',
        ],
      },
      {
        name: 'commit-derived-statuses-and-totals',
        capturePath: '$.commit.response.data.orderEditCommit.order',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-derived-statuses-commit.graphql',
          variables: {
            id: {
              fromProxyResponse: 'begin-edit-payload',
              path: '$.data.orderEditBegin.calculatedOrder.id',
            },
            notifyCustomer: false,
            staffNote: {
              fromCapturePath: '$.commit.variables.staffNote',
            },
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.orderEditCommit.order',
        excludedPaths: ['$.id'],
      },
      {
        name: 'downstream-derived-statuses-and-totals',
        capturePath: '$.downstreamRead.response.data.order',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-derived-statuses-read.graphql',
          variables: {
            id: {
              fromProxyResponse: 'draft-complete-paid-order-state',
              path: '$.data.draftOrderComplete.draftOrder.order.id',
            },
          },
          apiVersion: cap.apiVersion,
        },
        proxyPath: '$.data.order',
        excludedPaths: ['$.id'],
      },
    ],
  },
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      specPath,
      draftId,
      orderId,
      calcId,
      cleanupStatus: cleanup.status,
      commitOrder: readRecord(readRecord(readRecord(commit['data'])?.['orderEditCommit'])?.['order']),
      downstreamOrder: readRecord(readRecord(downstreamRead['data'])?.['order']),
    },
    null,
    2,
  ),
);
