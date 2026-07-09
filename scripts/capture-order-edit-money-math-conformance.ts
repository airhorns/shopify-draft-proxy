/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

const cap = await createConformanceCapture();
const expectedApiVersion = '2026-04';
if (cap.apiVersion !== expectedApiVersion) {
  throw new Error(`order-edit-money-math requires SHOPIFY_CONFORMANCE_API_VERSION=${expectedApiVersion}`);
}

const scenarioId = 'order-edit-money-math';
const fixturePath = cap.fixturePath('orders', 'order-edit-money-math.json');

const createDocument = await cap.readRequest('orders', 'orderEdit-money-math-create.graphql');
const beginDocument = await cap.readRequest('orders', 'orderEdit-money-math-begin.graphql');
const setQuantityDocument = await cap.readRequest('orders', 'orderEdit-money-math-setQuantity.graphql');
const addLineItemDiscountDocument = await cap.readRequest('orders', 'orderEdit-money-math-addLineItemDiscount.graphql');
const commitDocument = await cap.readRequest('orders', 'orderEdit-money-math-commit.graphql');
const readDocument = await cap.readRequest('orders', 'orderEdit-money-math-read.graphql');

const cancelDocument = `#graphql
  mutation OrderEditMoneyMathCancel($orderId: ID!, $reason: OrderCancelReason!, $notifyCustomer: Boolean!, $restock: Boolean!) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
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

type CaptureStep = {
  query: string;
  variables: JsonRecord;
  response: {
    status: number;
    payload: JsonRecord;
  };
};

function moneySet(amount: string, currencyCode = 'CAD'): JsonRecord {
  return {
    shopMoney: { amount, currencyCode },
    presentmentMoney: { amount, currencyCode },
  };
}

async function captureStep(query: string, variables: JsonRecord, label: string): Promise<CaptureStep> {
  const response = await cap.runGraphqlRequest<JsonRecord>(query, variables);
  const payload = response.payload as JsonRecord;
  if (response.status < 200 || response.status >= 300 || payload['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(response, null, 2)}`);
  }
  return {
    query,
    variables,
    response: {
      status: response.status,
      payload,
    },
  };
}

function mutationRoot(step: CaptureStep, rootName: string, label: string): JsonRecord {
  const root = readRecord(readRecord(step.response.payload['data'])?.[rootName]);
  if (!root) {
    throw new Error(`${label} missing ${rootName}: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  const userErrors = readArray(root['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${label} ${rootName} userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
  return root;
}

function queryRoot(step: CaptureStep, rootName: string, label: string): JsonRecord {
  const root = readRecord(readRecord(step.response.payload['data'])?.[rootName]);
  if (!root) {
    throw new Error(`${label} missing ${rootName}: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return root;
}

function orderIdFromCreate(step: CaptureStep, label: string): string {
  const order = readRecord(mutationRoot(step, 'orderCreate', label)['order']);
  return requireString(order?.['id'], `${label} order id`);
}

function calculatedOrderFromBegin(step: CaptureStep, label: string): JsonRecord {
  const calculatedOrder = readRecord(mutationRoot(step, 'orderEditBegin', label)['calculatedOrder']);
  if (!calculatedOrder) {
    throw new Error(`${label} missing calculatedOrder: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return calculatedOrder;
}

function calculatedOrderIdFromBegin(step: CaptureStep, label: string): string {
  return requireString(calculatedOrderFromBegin(step, label)['id'], `${label} calculated order id`);
}

function firstCalculatedLineItemIdFromBegin(step: CaptureStep, label: string): string {
  const calculatedOrder = calculatedOrderFromBegin(step, label);
  const lineItems = readRecord(calculatedOrder['lineItems']);
  const nodes = readArray(lineItems?.['nodes']);
  const first = readRecord(nodes[0]);
  return requireString(first?.['id'], `${label} first calculated line item id`);
}

function orderOptions(): JsonRecord {
  return {
    inventoryBehaviour: 'BYPASS',
    sendReceipt: false,
    sendFulfillmentReceipt: false,
  };
}

function shippingAddress(): JsonRecord {
  return {
    firstName: 'Conformance',
    lastName: 'OrderEdit',
    address1: '123 Queen St W',
    city: 'Toronto',
    provinceCode: 'ON',
    countryCode: 'CA',
    zip: 'M5H 2M9',
  };
}

function taxedOrderVariables(): JsonRecord {
  return {
    order: {
      email: `order-edit-money-taxed-${cap.stamp}@example.com`,
      note: `order edit money math taxed ${cap.stamp}`,
      tags: ['order-edit-money-math', 'taxed', cap.stamp],
      test: true,
      currency: 'CAD',
      financialStatus: 'PENDING',
      shippingAddress: shippingAddress(),
      lineItems: [
        {
          title: `Order edit taxed source ${cap.stamp}`,
          quantity: 2,
          priceSet: moneySet('100.00'),
          requiresShipping: true,
          taxable: true,
          sku: `order-edit-taxed-${cap.stamp}`,
          taxLines: [
            {
              title: 'Line tax',
              rate: 0.13,
              priceSet: moneySet('26.00'),
            },
          ],
        },
      ],
    },
    options: orderOptions(),
  };
}

function discountOrderVariables(): JsonRecord {
  return {
    order: {
      email: `order-edit-money-discount-${cap.stamp}@example.com`,
      note: `order edit money math discount ${cap.stamp}`,
      tags: ['order-edit-money-math', 'discount', cap.stamp],
      test: true,
      currency: 'CAD',
      financialStatus: 'PENDING',
      shippingAddress: shippingAddress(),
      lineItems: [
        {
          title: `Order edit discount source ${cap.stamp}`,
          quantity: 1,
          priceSet: moneySet('100.00'),
          requiresShipping: true,
          taxable: false,
          sku: `order-edit-discount-${cap.stamp}`,
        },
      ],
    },
    options: orderOptions(),
  };
}

const createdOrderIds: string[] = [];
const cleanup: JsonRecord[] = [];

async function cleanupOrders(): Promise<void> {
  for (const orderId of [...new Set(createdOrderIds)].reverse()) {
    if (cleanup.some((entry) => entry['orderId'] === orderId)) {
      continue;
    }
    try {
      cleanup.push({
        orderId,
        result: await cap.runGraphqlRequest(cancelDocument, {
          orderId,
          reason: 'OTHER',
          notifyCustomer: false,
          restock: false,
        }),
      });
    } catch (error) {
      cleanup.push({ orderId, error: (error as Error).message });
    }
  }
}

let fixtureWritten = false;

try {
  const taxedCreate = await captureStep(createDocument, taxedOrderVariables(), 'taxed orderCreate');
  const taxedOrderId = orderIdFromCreate(taxedCreate, 'taxed orderCreate');
  createdOrderIds.push(taxedOrderId);

  const taxedBegin = await captureStep(beginDocument, { id: taxedOrderId }, 'taxed orderEditBegin');
  const taxedCalculatedOrderId = calculatedOrderIdFromBegin(taxedBegin, 'taxed orderEditBegin');
  const taxedLineItemId = firstCalculatedLineItemIdFromBegin(taxedBegin, 'taxed orderEditBegin');

  const taxedSetQuantity = await captureStep(
    setQuantityDocument,
    { id: taxedCalculatedOrderId, lineItemId: taxedLineItemId, quantity: 1 },
    'taxed orderEditSetQuantity',
  );
  mutationRoot(taxedSetQuantity, 'orderEditSetQuantity', 'taxed orderEditSetQuantity');

  const taxedCommit = await captureStep(
    commitDocument,
    {
      id: taxedCalculatedOrderId,
      notifyCustomer: false,
      staffNote: 'order edit money math taxed commit',
    },
    'taxed orderEditCommit',
  );
  mutationRoot(taxedCommit, 'orderEditCommit', 'taxed orderEditCommit');

  const taxedDownstreamRead = await captureStep(readDocument, { id: taxedOrderId }, 'taxed downstream order read');
  queryRoot(taxedDownstreamRead, 'order', 'taxed downstream order read');

  const discountCreate = await captureStep(createDocument, discountOrderVariables(), 'discount orderCreate');
  const discountOrderId = orderIdFromCreate(discountCreate, 'discount orderCreate');
  createdOrderIds.push(discountOrderId);

  const discountBegin = await captureStep(beginDocument, { id: discountOrderId }, 'discount orderEditBegin');
  const discountCalculatedOrderId = calculatedOrderIdFromBegin(discountBegin, 'discount orderEditBegin');
  const discountLineItemId = firstCalculatedLineItemIdFromBegin(discountBegin, 'discount orderEditBegin');

  const addLineItemDiscount = await captureStep(
    addLineItemDiscountDocument,
    {
      id: discountCalculatedOrderId,
      lineItemId: discountLineItemId,
      discount: {
        description: 'order edit money math line discount',
        fixedValue: {
          amount: '20.00',
          currencyCode: 'CAD',
        },
      },
    },
    'discount orderEditAddLineItemDiscount',
  );
  mutationRoot(addLineItemDiscount, 'orderEditAddLineItemDiscount', 'discount orderEditAddLineItemDiscount');

  const discountCommit = await captureStep(
    commitDocument,
    {
      id: discountCalculatedOrderId,
      notifyCustomer: false,
      staffNote: 'order edit money math discount commit',
    },
    'discount orderEditCommit',
  );
  mutationRoot(discountCommit, 'orderEditCommit', 'discount orderEditCommit');

  const discountDownstreamRead = await captureStep(
    readDocument,
    { id: discountOrderId },
    'discount downstream order read',
  );
  queryRoot(discountDownstreamRead, 'order', 'discount downstream order read');

  await cleanupOrders();

  await cap.writeJson(fixturePath, {
    capturedAt: new Date().toISOString(),
    scenarioId,
    source: 'live-shopify-admin-graphql',
    storeDomain: cap.storeDomain,
    apiVersion: cap.apiVersion,
    notes:
      'Live Shopify Admin GraphQL capture for order-edit money math. The taxed branch creates a disposable two-unit order with a $26 line tax, sets quantity to one, commits, and reads the order back. The discount branch creates a disposable zero-tax order, adds a fixed $20 line-item discount, commits, and reads the order back. All setup uses public Admin GraphQL requests; no proxy/local-runtime output is used as Shopify evidence.',
    cases: {
      taxed: {
        create: taxedCreate,
        begin: taxedBegin,
        setQuantity: taxedSetQuantity,
        commit: taxedCommit,
        downstreamRead: taxedDownstreamRead,
      },
      discount: {
        create: discountCreate,
        begin: discountBegin,
        addLineItemDiscount,
        commit: discountCommit,
        downstreamRead: discountDownstreamRead,
      },
    },
    cleanup,
    upstreamCalls: [],
  });
  fixtureWritten = true;

  console.log(
    JSON.stringify(
      {
        ok: true,
        fixturePath,
        orderIds: createdOrderIds,
      },
      null,
      2,
    ),
  );
} finally {
  if (!fixtureWritten) {
    await cleanupOrders();
  }
}
