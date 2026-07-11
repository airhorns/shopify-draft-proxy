/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
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

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'admin-platform');
const fixturePath = path.join(fixtureDir, 'fulfillment-return-node-read-after-write.json');

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  const response = await runGraphqlRequest(query, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`GraphQL capture failed: ${JSON.stringify(response.payload, null, 2)}`);
  }
  return { query: trimGraphql(query), variables, response };
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

function dataRecord(captureResult: GraphqlCapture): JsonRecord {
  return readRecord(captureResult.response.payload)?.['data'] as JsonRecord;
}

function rootRecord(captureResult: GraphqlCapture, rootName: string): JsonRecord {
  return readRecord(dataRecord(captureResult)?.[rootName]) ?? {};
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing required capture value: ${label}`);
  }
  return value;
}

function requireEmptyUserErrors(captureResult: GraphqlCapture, rootName: string): void {
  const root = rootRecord(captureResult, rootName);
  const userErrors = readArray(root['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`Unexpected ${rootName} userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

const orderCreateMutation = `#graphql
  mutation FulfillmentReturnNodeOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFulfillmentStatus
        lineItems(first: 5) {
          nodes { id title quantity }
        }
        fulfillmentOrders(first: 5) {
          nodes {
            id
            status
            requestStatus
            lineItems(first: 5) {
              nodes {
                id
                totalQuantity
                remainingQuantity
                lineItem { id title }
              }
            }
          }
        }
      }
      userErrors { field message code }
    }
  }
`;

const fulfillmentOrderHoldMutation = `#graphql
  mutation FulfillmentReturnNodeHold($id: ID!, $fulfillmentHold: FulfillmentOrderHoldInput!) {
    fulfillmentOrderHold(id: $id, fulfillmentHold: $fulfillmentHold) {
      fulfillmentHold { id handle reason displayReason reasonNotes }
      fulfillmentOrder {
        id
        status
        requestStatus
        fulfillmentHolds { id handle reason displayReason reasonNotes }
        lineItems(first: 5) {
          nodes { id totalQuantity remainingQuantity lineItem { id title } }
        }
      }
      userErrors { field message code }
    }
  }
`;

const fulfillmentOrderNodeReadQuery = `#graphql
  query FulfillmentReturnNodeFulfillmentOrderRead($fulfillmentOrderId: ID!, $holdId: ID!, $lineItemId: ID!) {
    fulfillmentOrderNode: node(id: $fulfillmentOrderId) {
      __typename
      ... on FulfillmentOrder {
        id
        status
        requestStatus
        order { id }
        fulfillmentHolds { id handle reason displayReason reasonNotes }
        lineItems(first: 5) {
          nodes { id totalQuantity remainingQuantity lineItem { id title } }
        }
      }
    }
    fulfillmentHoldNode: node(id: $holdId) {
      __typename
      ... on FulfillmentHold { id handle reason displayReason reasonNotes }
    }
    fulfillmentOrderLineItemNode: node(id: $lineItemId) {
      __typename
      ... on FulfillmentOrderLineItem {
        id
        totalQuantity
        remainingQuantity
        lineItem { id title }
      }
    }
    duplicateNodes: nodes(ids: [$fulfillmentOrderId, $lineItemId, $fulfillmentOrderId]) {
      __typename
      ... on FulfillmentOrder { id status requestStatus }
      ... on FulfillmentOrderLineItem { id totalQuantity remainingQuantity }
    }
  }
`;

const fulfillmentOrderReleaseHoldMutation = `#graphql
  mutation FulfillmentReturnNodeReleaseHold($id: ID!, $holdIds: [ID!]) {
    fulfillmentOrderReleaseHold(id: $id, holdIds: $holdIds) {
      fulfillmentOrder { id status requestStatus fulfillmentHolds { id } }
      userErrors { field message code }
    }
  }
`;

const releasedHoldNodeReadQuery = `#graphql
  query FulfillmentReturnNodeReleasedHoldRead($holdId: ID!) {
    releasedHoldNode: node(id: $holdId) {
      __typename
      ... on FulfillmentHold { id }
    }
  }
`;

const fulfillmentCreateMutation = `#graphql
  mutation FulfillmentReturnNodeFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
    fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
      fulfillment {
        id
        status
        displayStatus
        trackingInfo(first: 5) { number url company }
        fulfillmentLineItems(first: 5) {
          nodes { id quantity lineItem { id title } }
        }
      }
      userErrors { field message }
    }
  }
`;

const fulfillmentEventCreateMutation = `#graphql
  mutation FulfillmentReturnNodeFulfillmentEventCreate($fulfillmentEvent: FulfillmentEventInput!) {
    fulfillmentEventCreate(fulfillmentEvent: $fulfillmentEvent) {
      fulfillmentEvent { id status message }
      userErrors { field message }
    }
  }
`;

const fulfillmentTrackingUpdateMutation = `#graphql
  mutation FulfillmentReturnNodeTrackingUpdate(
    $fulfillmentId: ID!
    $trackingInfoInput: FulfillmentTrackingInput!
    $notifyCustomer: Boolean
  ) {
    fulfillmentTrackingInfoUpdate(
      fulfillmentId: $fulfillmentId
      trackingInfoInput: $trackingInfoInput
      notifyCustomer: $notifyCustomer
    ) {
      fulfillment {
        id
        status
        displayStatus
        trackingInfo(first: 5) { number url company }
        events(first: 5) { nodes { id status message } }
      }
      userErrors { field message }
    }
  }
`;

const fulfillmentNodeReadQuery = `#graphql
  query FulfillmentReturnNodeFulfillmentRead(
    $fulfillmentId: ID!
    $fulfillmentLineItemId: ID!
    $fulfillmentEventId: ID!
  ) {
    fulfillmentNode: node(id: $fulfillmentId) {
      __typename
      ... on Fulfillment {
        id
        status
        displayStatus
        order { id }
        trackingInfo(first: 5) { number url company }
        events(first: 5) { nodes { id status message } }
        fulfillmentLineItems(first: 5) {
          nodes { id quantity lineItem { id title } }
        }
      }
    }
    fulfillmentLineItemNode: node(id: $fulfillmentLineItemId) {
      __typename
      ... on FulfillmentLineItem {
        id
        quantity
        lineItem { id title }
      }
    }
    fulfillmentEventNode: node(id: $fulfillmentEventId) {
      __typename
      ... on FulfillmentEvent { id status message }
    }
    duplicateNodes: nodes(ids: [$fulfillmentId, $fulfillmentLineItemId, $fulfillmentId, $fulfillmentEventId]) {
      __typename
      ... on Fulfillment { id status displayStatus }
      ... on FulfillmentLineItem { id quantity }
      ... on FulfillmentEvent { id status message }
    }
  }
`;

const returnableFulfillmentsQuery = `#graphql
  query FulfillmentReturnNodeReturnableFulfillments($orderId: ID!) {
    returnableFulfillments(orderId: $orderId, first: 5) {
      nodes {
        id
        fulfillment { id }
        returnableFulfillmentLineItems(first: 5) {
          nodes {
            quantity
            fulfillmentLineItem {
              id
              quantity
              lineItem { id title }
            }
          }
        }
      }
    }
  }
`;

const returnableFulfillmentNodeReadQuery = `#graphql
  query FulfillmentReturnNodeReturnableFulfillmentRead($returnableFulfillmentId: ID!) {
    returnableFulfillmentNode: node(id: $returnableFulfillmentId) {
      __typename
      ... on ReturnableFulfillment {
        id
        fulfillment { id }
        returnableFulfillmentLineItems(first: 5) {
          nodes {
            quantity
            fulfillmentLineItem {
              id
              quantity
              lineItem { id title }
            }
          }
        }
      }
    }
  }
`;

const returnRequestMutation = `#graphql
  mutation FulfillmentReturnNodeReturnRequest($input: ReturnRequestInput!) {
    returnRequest(input: $input) {
      return {
        id
        name
        status
        totalQuantity
        returnLineItems(first: 5) {
          nodes {
            id
            quantity
            processableQuantity
            processedQuantity
            unprocessedQuantity
          }
        }
        reverseFulfillmentOrders(first: 5) { nodes { id } }
      }
      userErrors { field message }
    }
  }
`;

const returnApproveMutation = `#graphql
  mutation FulfillmentReturnNodeReturnApprove($input: ReturnApproveRequestInput!) {
    returnApproveRequest(input: $input) {
      return {
        id
        name
        status
        totalQuantity
        returnLineItems(first: 5) {
          nodes {
            id
            quantity
            processableQuantity
            processedQuantity
            unprocessedQuantity
          }
        }
        reverseFulfillmentOrders(first: 5) {
          nodes {
            id
            status
            lineItems(first: 5) {
              nodes {
                id
                totalQuantity
                fulfillmentLineItem { id quantity lineItem { id title } }
              }
            }
          }
        }
      }
      userErrors { field message code }
    }
  }
`;

const returnReverseNodeReadQuery = `#graphql
  query FulfillmentReturnNodeReturnReverseRead(
    $returnId: ID!
    $returnLineItemId: ID!
    $reverseFulfillmentOrderId: ID!
    $reverseFulfillmentOrderLineItemId: ID!
    $reverseDeliveryId: ID!
    $reverseDeliveryLineItemId: ID!
  ) {
    returnNode: node(id: $returnId) {
      __typename
      ... on Return {
        id
        name
        status
        totalQuantity
        returnLineItems(first: 5) {
          nodes {
            id
            quantity
            processableQuantity
            processedQuantity
            unprocessedQuantity
          }
        }
        reverseFulfillmentOrders(first: 5) {
          nodes {
            id
            status
            lineItems(first: 5) {
              nodes { id totalQuantity fulfillmentLineItem { id } }
            }
            reverseDeliveries(first: 5) { nodes { id } }
          }
        }
      }
    }
    returnLineItemNode: node(id: $returnLineItemId) {
      __typename
      ... on ReturnLineItem {
        id
        quantity
        processableQuantity
        processedQuantity
        unprocessedQuantity
        fulfillmentLineItem { id quantity lineItem { id title } }
      }
    }
    reverseFulfillmentOrderNode: node(id: $reverseFulfillmentOrderId) {
      __typename
      ... on ReverseFulfillmentOrder {
        id
        status
        order { id }
        lineItems(first: 5) {
          nodes { id totalQuantity fulfillmentLineItem { id } }
        }
        reverseDeliveries(first: 5) { nodes { id } }
      }
    }
    reverseFulfillmentOrderLineItemNode: node(id: $reverseFulfillmentOrderLineItemId) {
      __typename
      ... on ReverseFulfillmentOrderLineItem {
        id
        totalQuantity
        fulfillmentLineItem { id }
      }
    }
    reverseDeliveryNode: node(id: $reverseDeliveryId) {
      __typename
      ... on ReverseDelivery {
        id
        reverseFulfillmentOrder { id status }
        reverseDeliveryLineItems(first: 5) {
          nodes {
            id
            quantity
            reverseFulfillmentOrderLineItem { id }
          }
        }
        deliverable {
          __typename
          ... on ReverseDeliveryShippingDeliverable {
            tracking { number url carrierName }
          }
        }
      }
    }
    reverseDeliveryLineItemNode: node(id: $reverseDeliveryLineItemId) {
      __typename
      ... on ReverseDeliveryLineItem {
        id
        quantity
        reverseFulfillmentOrderLineItem { id }
      }
    }
    duplicateNodes: nodes(ids: [$returnId, $reverseDeliveryId, $returnId, $reverseFulfillmentOrderId]) {
      __typename
      ... on Return { id status totalQuantity }
      ... on ReverseDelivery { id reverseFulfillmentOrder { id } }
      ... on ReverseFulfillmentOrder { id status }
    }
  }
`;

const reverseDeliveryCreateMutation = `#graphql
  mutation FulfillmentReturnNodeReverseDeliveryCreate(
    $reverseFulfillmentOrderId: ID!
    $reverseDeliveryLineItems: [ReverseDeliveryLineItemInput!]!
    $trackingInput: ReverseDeliveryTrackingInput
    $labelInput: ReverseDeliveryLabelInput
  ) {
    reverseDeliveryCreateWithShipping(
      reverseFulfillmentOrderId: $reverseFulfillmentOrderId
      reverseDeliveryLineItems: $reverseDeliveryLineItems
      trackingInput: $trackingInput
      labelInput: $labelInput
      notifyCustomer: true
    ) {
      reverseDelivery {
        id
        reverseFulfillmentOrder { id status }
        reverseDeliveryLineItems(first: 5) {
          nodes {
            id
            quantity
            reverseFulfillmentOrderLineItem { id totalQuantity }
          }
        }
        deliverable {
          __typename
          ... on ReverseDeliveryShippingDeliverable {
            tracking { number url carrierName }
            label { publicFileUrl }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const reverseDeliveryUpdateMutation = `#graphql
  mutation FulfillmentReturnNodeReverseDeliveryUpdate($reverseDeliveryId: ID!, $trackingInput: ReverseDeliveryTrackingInput) {
    reverseDeliveryShippingUpdate(reverseDeliveryId: $reverseDeliveryId, trackingInput: $trackingInput) {
      reverseDelivery {
        id
        deliverable {
          __typename
          ... on ReverseDeliveryShippingDeliverable {
            tracking { number url carrierName }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const returnCloseMutation = `#graphql
  mutation FulfillmentReturnNodeReturnClose($id: ID!) {
    returnClose(id: $id) {
      return { id status closedAt }
      userErrors { field message }
    }
  }
`;

const returnReopenMutation = `#graphql
  mutation FulfillmentReturnNodeReturnReopen($id: ID!) {
    returnReopen(id: $id) {
      return { id status closedAt }
      userErrors { field message }
    }
  }
`;

const returnStatusNodeReadQuery = `#graphql
  query FulfillmentReturnNodeReturnStatusRead($returnId: ID!) {
    returnNode: node(id: $returnId) {
      __typename
      ... on Return { id status closedAt }
    }
  }
`;

const missingNodesQuery = `#graphql
  query FulfillmentReturnNodeMissingRead {
    missingNodes: nodes(ids: [
      "gid://shopify/Fulfillment/999999999999999"
      "gid://shopify/FulfillmentEvent/999999999999999"
      "gid://shopify/FulfillmentLineItem/999999999999999"
      "gid://shopify/FulfillmentOrder/999999999999999"
      "gid://shopify/FulfillmentHold/999999999999999"
      "gid://shopify/FulfillmentOrderLineItem/999999999999999"
      "gid://shopify/Return/999999999999999"
      "gid://shopify/ReturnableFulfillment/999999999999999"
      "gid://shopify/ReturnLineItem/999999999999999"
      "gid://shopify/UnverifiedReturnLineItem/999999999999999"
      "gid://shopify/ReverseDelivery/999999999999999"
      "gid://shopify/ReverseDeliveryLineItem/999999999999999"
      "gid://shopify/ReverseFulfillmentOrder/999999999999999"
      "gid://shopify/ReverseFulfillmentOrderLineItem/999999999999999"
    ]) {
      __typename
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation FulfillmentReturnNodeCleanupOrderCancel(
    $orderId: ID!
    $notifyCustomer: Boolean
    $reason: OrderCancelReason!
    $refund: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(
      orderId: $orderId
      notifyCustomer: $notifyCustomer
      reason: $reason
      refund: $refund
      restock: $restock
    ) {
      job { id done }
      orderCancelUserErrors { field message code }
      userErrors { field message }
    }
  }
`;

const stamp = Date.now();
const orderCreate = await capture(orderCreateMutation, {
  order: {
    email: `fulfillment-return-node-${stamp}@example.com`,
    note: `fulfillment return node capture ${stamp}`,
    tags: ['parity-probe', 'fulfillment-return-node'],
    test: true,
    lineItems: [
      {
        title: `Fulfillment return node line ${stamp}`,
        quantity: 2,
        priceSet: {
          shopMoney: { amount: '14.00', currencyCode: 'CAD' },
        },
        requiresShipping: true,
        taxable: false,
        sku: `NODE-${stamp}`,
      },
    ],
    transactions: [
      {
        kind: 'SALE',
        status: 'SUCCESS',
        gateway: 'manual',
        test: true,
        amountSet: {
          shopMoney: { amount: '28.00', currencyCode: 'CAD' },
        },
      },
    ],
  },
  options: null,
});
requireEmptyUserErrors(orderCreate, 'orderCreate');

const createdOrder = readRecord(rootRecord(orderCreate, 'orderCreate')['order']) ?? {};
const orderId = requireString(createdOrder['id'], 'order id');
const fulfillmentOrder = readNodes(createdOrder['fulfillmentOrders'])[0] ?? {};
const fulfillmentOrderId = requireString(fulfillmentOrder['id'], 'fulfillment order id');
const fulfillmentOrderLineItem = readNodes(fulfillmentOrder['lineItems'])[0] ?? {};
const fulfillmentOrderLineItemId = requireString(fulfillmentOrderLineItem['id'], 'fulfillment order line item id');

const fulfillmentOrderHold = await capture(fulfillmentOrderHoldMutation, {
  id: fulfillmentOrderId,
  fulfillmentHold: {
    reason: 'AWAITING_RETURN_ITEMS',
    reasonNotes: 'generic Node hold coverage',
    handle: `node-hold-${stamp}`,
  },
});
requireEmptyUserErrors(fulfillmentOrderHold, 'fulfillmentOrderHold');

const fulfillmentHold = readRecord(rootRecord(fulfillmentOrderHold, 'fulfillmentOrderHold')['fulfillmentHold']) ?? {};
const fulfillmentHoldId = requireString(fulfillmentHold['id'], 'fulfillment hold id');
const fulfillmentOrderNodeRead = await capture(fulfillmentOrderNodeReadQuery, {
  fulfillmentOrderId,
  holdId: fulfillmentHoldId,
  lineItemId: fulfillmentOrderLineItemId,
});

const fulfillmentOrderReleaseHold = await capture(fulfillmentOrderReleaseHoldMutation, {
  id: fulfillmentOrderId,
  holdIds: [fulfillmentHoldId],
});
requireEmptyUserErrors(fulfillmentOrderReleaseHold, 'fulfillmentOrderReleaseHold');
const releasedHoldNodeRead = await capture(releasedHoldNodeReadQuery, { holdId: fulfillmentHoldId });

const trackingNumber = `NODE-CREATE-${stamp}`;
const updatedTrackingNumber = `NODE-UPDATED-${stamp}`;
const fulfillmentCreate = await capture(fulfillmentCreateMutation, {
  fulfillment: {
    notifyCustomer: false,
    trackingInfo: {
      number: trackingNumber,
      url: `https://example.com/track/${trackingNumber}`,
      company: 'Hermes',
    },
    lineItemsByFulfillmentOrder: [{ fulfillmentOrderId }],
  },
  message: 'generic Node fulfillment capture',
});
requireEmptyUserErrors(fulfillmentCreate, 'fulfillmentCreate');

const fulfillment = readRecord(rootRecord(fulfillmentCreate, 'fulfillmentCreate')['fulfillment']) ?? {};
const fulfillmentId = requireString(fulfillment['id'], 'fulfillment id');
const fulfillmentLineItem = readNodes(fulfillment['fulfillmentLineItems'])[0] ?? {};
const fulfillmentLineItemId = requireString(fulfillmentLineItem['id'], 'fulfillment line item id');

const fulfillmentEventCreate = await capture(fulfillmentEventCreateMutation, {
  fulfillmentEvent: {
    fulfillmentId,
    status: 'IN_TRANSIT',
    message: 'Generic Node event capture',
  },
});
requireEmptyUserErrors(fulfillmentEventCreate, 'fulfillmentEventCreate');
const fulfillmentEvent =
  readRecord(rootRecord(fulfillmentEventCreate, 'fulfillmentEventCreate')['fulfillmentEvent']) ?? {};
const fulfillmentEventId = requireString(fulfillmentEvent['id'], 'fulfillment event id');

const fulfillmentTrackingUpdate = await capture(fulfillmentTrackingUpdateMutation, {
  fulfillmentId,
  notifyCustomer: false,
  trackingInfoInput: {
    number: updatedTrackingNumber,
    url: `https://example.com/track/${updatedTrackingNumber}`,
    company: 'Hermes Updated',
  },
});
requireEmptyUserErrors(fulfillmentTrackingUpdate, 'fulfillmentTrackingInfoUpdate');
const fulfillmentNodeRead = await capture(fulfillmentNodeReadQuery, {
  fulfillmentId,
  fulfillmentLineItemId,
  fulfillmentEventId,
});

const returnableFulfillments = await capture(returnableFulfillmentsQuery, { orderId });
const returnableFulfillment = readNodes(dataRecord(returnableFulfillments)['returnableFulfillments'])[0] ?? {};
const returnableFulfillmentId = requireString(returnableFulfillment['id'], 'returnable fulfillment id');
const returnableFulfillmentNodeRead = await capture(returnableFulfillmentNodeReadQuery, { returnableFulfillmentId });

const returnRequest = await capture(returnRequestMutation, {
  input: {
    orderId,
    returnLineItems: [
      {
        fulfillmentLineItemId,
        quantity: 1,
        returnReason: 'OTHER',
        customerNote: 'Generic Node return coverage',
      },
    ],
  },
});
requireEmptyUserErrors(returnRequest, 'returnRequest');
const requestedReturn = readRecord(rootRecord(returnRequest, 'returnRequest')['return']) ?? {};
const returnId = requireString(requestedReturn['id'], 'return id');
const returnLineItem = readNodes(requestedReturn['returnLineItems'])[0] ?? {};
const returnLineItemId = requireString(returnLineItem['id'], 'return line item id');

const returnApproveRequest = await capture(returnApproveMutation, { input: { id: returnId } });
requireEmptyUserErrors(returnApproveRequest, 'returnApproveRequest');
const approvedReturn = readRecord(rootRecord(returnApproveRequest, 'returnApproveRequest')['return']) ?? {};
const reverseFulfillmentOrder = readNodes(approvedReturn['reverseFulfillmentOrders'])[0] ?? {};
const reverseFulfillmentOrderId = requireString(reverseFulfillmentOrder['id'], 'reverse fulfillment order id');
const reverseFulfillmentOrderLineItem = readNodes(reverseFulfillmentOrder['lineItems'])[0] ?? {};
const reverseFulfillmentOrderLineItemId = requireString(
  reverseFulfillmentOrderLineItem['id'],
  'reverse fulfillment order line item id',
);

const trackingInput = {
  number: `RNODE-${stamp}`,
  url: `https://example.com/reverse/${stamp}`,
};
const updatedTrackingInput = {
  number: `RNODE-UPDATED-${stamp}`,
  url: `https://example.com/reverse-updated/${stamp}`,
};
const labelInput = { fileUrl: `https://example.com/reverse-label/${stamp}.pdf` };

const reverseDeliveryCreate = await capture(reverseDeliveryCreateMutation, {
  reverseFulfillmentOrderId,
  reverseDeliveryLineItems: [],
  trackingInput,
  labelInput,
});
requireEmptyUserErrors(reverseDeliveryCreate, 'reverseDeliveryCreateWithShipping');
const reverseDelivery =
  readRecord(rootRecord(reverseDeliveryCreate, 'reverseDeliveryCreateWithShipping')['reverseDelivery']) ?? {};
const reverseDeliveryId = requireString(reverseDelivery['id'], 'reverse delivery id');
const reverseDeliveryLineItem = readNodes(reverseDelivery['reverseDeliveryLineItems'])[0] ?? {};
const reverseDeliveryLineItemId = requireString(reverseDeliveryLineItem['id'], 'reverse delivery line item id');

const reverseDeliveryUpdate = await capture(reverseDeliveryUpdateMutation, {
  reverseDeliveryId,
  trackingInput: updatedTrackingInput,
});
requireEmptyUserErrors(reverseDeliveryUpdate, 'reverseDeliveryShippingUpdate');

const returnReverseNodeRead = await capture(returnReverseNodeReadQuery, {
  returnId,
  returnLineItemId,
  reverseFulfillmentOrderId,
  reverseFulfillmentOrderLineItemId,
  reverseDeliveryId,
  reverseDeliveryLineItemId,
});

const returnClose = await capture(returnCloseMutation, { id: returnId });
requireEmptyUserErrors(returnClose, 'returnClose');
const returnClosedNodeRead = await capture(returnStatusNodeReadQuery, { returnId });

const returnReopen = await capture(returnReopenMutation, { id: returnId });
requireEmptyUserErrors(returnReopen, 'returnReopen');
const returnReopenedNodeRead = await capture(returnStatusNodeReadQuery, { returnId });

const missingNodes = await capture(missingNodesQuery);

let cleanup: GraphqlCapture;
try {
  cleanup = await capture(orderCancelMutation, {
    orderId,
    notifyCustomer: false,
    reason: 'OTHER',
    refund: true,
    restock: true,
  });
} catch (error) {
  cleanup = {
    query: trimGraphql(orderCancelMutation),
    variables: {
      orderId,
      notifyCustomer: false,
      reason: 'OTHER',
      refund: true,
      restock: true,
    },
    response: {
      status: 0,
      payload: { cleanupError: error instanceof Error ? error.message : String(error) },
    },
  };
}

await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  apiVersion,
  storeDomain,
  source: 'live-shopify-admin-graphql',
  notes:
    'Live Admin GraphQL generic Node evidence for staged fulfillment, fulfillment-order, return, and reverse-logistics resources. UnverifiedReturnLineItem is included in the missing-node read; this Admin API schema exposes the Node type but no public Return field or input path for a non-null instance.',
  setup: {
    orderCreate,
    fulfillmentOrderHold,
    fulfillmentOrderReleaseHold,
    fulfillmentCreate,
    fulfillmentEventCreate,
    fulfillmentTrackingUpdate,
    returnableFulfillments,
    returnRequest,
    returnApproveRequest,
    reverseDeliveryCreate,
    reverseDeliveryUpdate,
    returnClose,
    returnReopen,
  },
  nodeReads: {
    fulfillmentOrderNodeRead,
    releasedHoldNodeRead,
    fulfillmentNodeRead,
    returnableFulfillmentNodeRead,
    returnReverseNodeRead,
    returnClosedNodeRead,
    returnReopenedNodeRead,
    missingNodes,
  },
  cleanup,
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      orderId,
      fulfillmentOrderId,
      fulfillmentId,
      returnId,
      reverseDeliveryId,
      reverseFulfillmentOrderId,
      cleanupStatus: cleanup.response.status,
    },
    null,
    2,
  ),
);
