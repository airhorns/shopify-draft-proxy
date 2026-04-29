/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
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
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'return-reverse-logistics-recorded.json');
const requestDir = path.join('config', 'parity-requests', 'orders');

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  return {
    query: trimGraphql(query),
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readNodes(value: unknown): JsonRecord[] {
  return readArray(readRecord(value)?.['nodes'])
    .map(readRecord)
    .filter((node): node is JsonRecord => node !== null);
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing required capture value: ${label}`);
  }
  return value;
}

function requireEmptyUserErrors(captureResult: GraphqlCapture, rootName: string): void {
  const payload = captureResult.response.payload as JsonRecord;
  const errors = payload['errors'];
  const root = readRecord(readRecord(payload['data'])?.[rootName]);
  const userErrors = readArray(root?.['userErrors']);
  if (errors || userErrors.length > 0) {
    throw new Error(`Unexpected ${rootName} errors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function firstActiveLocationId(locations: GraphqlCapture): string {
  const nodes = readNodes(readRecord(readRecord(locations.response.payload)['data'])?.['locations']);
  const location = nodes.find((node) => node['isActive'] !== false) ?? nodes[0];
  return requireString(location?.['id'], 'location id');
}

function firstFulfillmentOrder(order: JsonRecord): JsonRecord {
  return readNodes(order['fulfillmentOrders'])[0] ?? {};
}

function firstFulfillmentLineItem(order: JsonRecord): JsonRecord {
  return readNodes(readRecord(readArray(order['fulfillments'])[0])?.['fulfillmentLineItems'])[0] ?? {};
}

function returnPayload(captureResult: GraphqlCapture, rootName: string): JsonRecord {
  return readRecord(readRecord(captureResult.response.payload as JsonRecord)['data'])?.[rootName] as JsonRecord;
}

const orderFields = `#graphql
  fragment ReturnReverseLogisticsOrderFields on Order {
    id
    name
    createdAt
    updatedAt
    displayFinancialStatus
    displayFulfillmentStatus
    totalPriceSet { shopMoney { amount currencyCode } }
    currentTotalPriceSet { shopMoney { amount currencyCode } }
    totalRefundedSet { shopMoney { amount currencyCode } }
    tags
    lineItems(first: 5) {
      nodes {
        id
        title
        quantity
        currentQuantity
      }
    }
    fulfillments(first: 5) {
      id
      status
      displayStatus
      createdAt
      updatedAt
      fulfillmentLineItems(first: 5) {
        nodes {
          id
          quantity
          lineItem {
            id
            title
          }
        }
      }
    }
    fulfillmentOrders(first: 5) {
      nodes {
        id
        status
        requestStatus
        assignedLocation {
          name
          location {
            id
          }
        }
        lineItems(first: 5) {
          nodes {
            id
            totalQuantity
            remainingQuantity
            lineItem {
              id
              title
            }
          }
        }
      }
    }
    returns(first: 5) {
      nodes {
        id
        name
        status
        totalQuantity
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  ${orderFields}
  mutation ReturnReverseLogisticsOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ...ReturnReverseLogisticsOrderFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentCreateMutation = `#graphql
  mutation ReturnReverseLogisticsFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
    fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
      fulfillment {
        id
        status
        displayStatus
        createdAt
        updatedAt
        fulfillmentLineItems(first: 5) {
          nodes {
            id
            quantity
            lineItem {
              id
              title
            }
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderReadQuery = `#graphql
  ${orderFields}
  query ReturnReverseLogisticsOrderRead($id: ID!) {
    order(id: $id) {
      ...ReturnReverseLogisticsOrderFields
    }
  }
`;

const locationsQuery = `#graphql
  query ReturnReverseLogisticsLocations($first: Int!) {
    locations(first: $first) {
      nodes {
        id
        name
        isActive
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation ReturnReverseLogisticsOrderCancel(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
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

const returnRequestMutation = await readRequest('return-request-recorded.graphql');
const returnApproveRequestMutation = await readRequest('return-approve-request-recorded.graphql');
const reverseDeliveryCreateMutation = await readRequest('reverse-delivery-create-with-shipping-recorded.graphql');
const reverseDeliveryUpdateMutation = await readRequest('reverse-delivery-shipping-update-recorded.graphql');
const reverseFulfillmentDisposeMutation = await readRequest('reverse-fulfillment-order-dispose-recorded.graphql');
const returnProcessMutation = await readRequest('return-process-recorded.graphql');
const downstreamReadQuery = await readRequest('return-reverse-logistics-read-recorded.graphql');

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const orderVariables = {
  order: {
    email: `har-442-return-reverse-${stamp}@example.com`,
    note: `HAR-442 return reverse logistics capture ${stamp}`,
    tags: ['har-442', 'return-reverse-logistics', stamp],
    test: true,
    currency: 'USD',
    shippingAddress: {
      firstName: 'HAR',
      lastName: 'Return',
      address1: '123 Queen St W',
      city: 'Toronto',
      provinceCode: 'ON',
      countryCode: 'CA',
      zip: 'M5H 2M9',
    },
    lineItems: [
      {
        variantId: 'gid://shopify/ProductVariant/48540157378793',
        title: `HAR-442 return item ${stamp}`,
        quantity: 2,
        priceSet: {
          shopMoney: {
            amount: '20.00',
            currencyCode: 'USD',
          },
        },
        requiresShipping: true,
        taxable: true,
      },
    ],
  },
  options: {
    inventoryBehaviour: 'BYPASS',
    sendReceipt: false,
    sendFulfillmentReceipt: false,
  },
};

const locations = await capture(locationsQuery, { first: 10 });
const locationId = firstActiveLocationId(locations);
const orderCreate = await capture(orderCreateMutation, orderVariables);
requireEmptyUserErrors(orderCreate, 'orderCreate');

const createdOrder = readRecord(returnPayload(orderCreate, 'orderCreate')['order']) ?? {};
const orderId = requireString(createdOrder['id'], 'created order id');
const fulfillmentOrder = firstFulfillmentOrder(createdOrder);
const fulfillmentOrderId = requireString(fulfillmentOrder['id'], 'created fulfillment order id');
const fulfillmentOrderLineItem = readNodes(fulfillmentOrder['lineItems'])[0] ?? {};
const fulfillmentOrderLineItemId = requireString(
  fulfillmentOrderLineItem['id'],
  'created fulfillment order line item id',
);

const fulfillmentCreate = await capture(fulfillmentCreateMutation, {
  fulfillment: {
    notifyCustomer: false,
    trackingInfo: {
      number: `HAR442-FULFILL-${stamp}`,
      url: `https://example.com/track/HAR442-FULFILL-${stamp}`,
      company: 'Hermes Carrier',
    },
    lineItemsByFulfillmentOrder: [
      {
        fulfillmentOrderId,
        fulfillmentOrderLineItems: [
          {
            id: fulfillmentOrderLineItemId,
            quantity: 2,
          },
        ],
      },
    ],
  },
  message: `HAR-442 return reverse logistics fulfillment ${stamp}`,
});
requireEmptyUserErrors(fulfillmentCreate, 'fulfillmentCreate');

const orderReadAfterFulfillment = await capture(orderReadQuery, { id: orderId });
const seedOrder = readRecord(readRecord(orderReadAfterFulfillment.response.payload)['data'])?.['order'];
const fulfillmentLineItem = firstFulfillmentLineItem(readRecord(seedOrder) ?? {});
const fulfillmentLineItemId = requireString(fulfillmentLineItem['id'], 'fulfilled fulfillment line item id');

const returnRequest = await capture(returnRequestMutation, {
  input: {
    orderId,
    returnLineItems: [
      {
        fulfillmentLineItemId,
        quantity: 1,
        returnReason: 'OTHER',
      },
    ],
  },
});
requireEmptyUserErrors(returnRequest, 'returnRequest');

const requestedReturn = readRecord(returnPayload(returnRequest, 'returnRequest')['return']) ?? {};
const returnId = requireString(requestedReturn['id'], 'requested return id');
const returnLineItem = readNodes(requestedReturn['returnLineItems'])[0] ?? {};
const returnLineItemId = requireString(returnLineItem['id'], 'return line item id');

const returnApproveRequest = await capture(returnApproveRequestMutation, {
  input: {
    id: returnId,
  },
});
requireEmptyUserErrors(returnApproveRequest, 'returnApproveRequest');

const approvedReturn = readRecord(returnPayload(returnApproveRequest, 'returnApproveRequest')['return']) ?? {};
const reverseFulfillmentOrder = readNodes(approvedReturn['reverseFulfillmentOrders'])[0] ?? {};
const reverseFulfillmentOrderId = requireString(reverseFulfillmentOrder['id'], 'reverse fulfillment order id');
const reverseFulfillmentOrderLineItem = readNodes(reverseFulfillmentOrder['lineItems'])[0] ?? {};
const reverseFulfillmentOrderLineItemId = requireString(
  reverseFulfillmentOrderLineItem['id'],
  'reverse fulfillment order line item id',
);

const trackingInput = {
  number: `HAR442-RETURN-${stamp}`,
  url: `https://example.com/returns/HAR442-RETURN-${stamp}`,
};
const updatedTrackingInput = {
  number: `HAR442-RETURN-UPDATED-${stamp}`,
  url: `https://example.com/returns/HAR442-RETURN-UPDATED-${stamp}`,
};
const labelInput = {
  fileUrl: `https://example.com/labels/HAR442-${stamp}.pdf`,
};

const reverseDeliveryCreate = await capture(reverseDeliveryCreateMutation, {
  reverseFulfillmentOrderId,
  reverseDeliveryLineItems: [],
  trackingInput,
  labelInput,
});
requireEmptyUserErrors(reverseDeliveryCreate, 'reverseDeliveryCreateWithShipping');

const reverseDelivery =
  readRecord(returnPayload(reverseDeliveryCreate, 'reverseDeliveryCreateWithShipping')['reverseDelivery']) ?? {};
const reverseDeliveryId = requireString(reverseDelivery['id'], 'reverse delivery id');

const reverseDeliveryUpdate = await capture(reverseDeliveryUpdateMutation, {
  reverseDeliveryId,
  trackingInput: updatedTrackingInput,
});
requireEmptyUserErrors(reverseDeliveryUpdate, 'reverseDeliveryShippingUpdate');

const reverseFulfillmentDispose = await capture(reverseFulfillmentDisposeMutation, {
  dispositionInputs: [
    {
      reverseFulfillmentOrderLineItemId,
      quantity: 1,
      dispositionType: 'NOT_RESTOCKED',
      locationId,
    },
  ],
});
requireEmptyUserErrors(reverseFulfillmentDispose, 'reverseFulfillmentOrderDispose');

const returnProcess = await capture(returnProcessMutation, {
  input: {
    returnId,
    returnLineItems: [
      {
        id: returnLineItemId,
        quantity: 1,
      },
    ],
    notifyCustomer: true,
  },
});
requireEmptyUserErrors(returnProcess, 'returnProcess');

const downstreamRead = await capture(downstreamReadQuery, {
  returnId,
  orderId,
  reverseDeliveryId,
  reverseFulfillmentOrderId,
});

const cleanup = await capture(orderCancelMutation, {
  orderId,
  reason: 'OTHER',
  notifyCustomer: false,
  restock: true,
});

await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  apiVersion,
  storeDomain,
  source: 'live-shopify-admin-graphql',
  notes:
    'HAR-442 live return/reverse-logistics capture. The replay request files are the same GraphQL documents used for return request approval, empty reverseDeliveryLineItems creation, fileUrl label input, shipping update, disposal, processing, and downstream reads.',
  locations,
  setup: {
    orderCreate,
    fulfillmentCreate,
    orderReadAfterFulfillment,
  },
  seedOrder,
  returnRequest,
  returnApproveRequest,
  reverseLogistics: {
    trackingInput,
    updatedTrackingInput,
    labelInput,
  },
  reverseDeliveryCreate,
  reverseDeliveryUpdate,
  reverseFulfillmentDispose,
  returnProcess,
  downstreamRead,
  cleanup,
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      orderId,
      returnId,
      reverseDeliveryId,
      reverseFulfillmentOrderId,
      cleanupUserErrors: readArray(readRecord(returnPayload(cleanup, 'orderCancel'))?.['userErrors']),
    },
    null,
    2,
  ),
);
