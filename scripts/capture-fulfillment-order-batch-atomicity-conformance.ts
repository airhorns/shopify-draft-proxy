/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { deepStrictEqual } from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
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

type CreatedOrder = {
  id: string;
  fulfillmentOrderId: string;
  fulfillmentOrderLineItemId: string;
};

type SplitPair = {
  primaryFulfillmentOrderId: string;
  siblingFulfillmentOrderId: string;
  siblingLineItemId: string;
};

const captureId = 'fulfillment-order-batch-atomicity';
const physicalVariantId = 'gid://shopify/ProductVariant/48540157378793';
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

const fixtureDirectory = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const fixturePath = path.join(fixtureDirectory, `${captureId}.json`);
const fixtureReference = fixturePath.replaceAll(path.sep, '/');
const paritySpecDirectory = path.join('config', 'parity-specs', 'shipping-fulfillments');
const parityRequestDirectory = path.join('config', 'parity-requests', 'shipping-fulfillments');
const splitSpecPath = path.join(paritySpecDirectory, 'fulfillment-order-split-batch-atomicity.json');
const mergeSpecPath = path.join(paritySpecDirectory, 'fulfillment-order-merge-batch-atomicity.json');
const orderReadRequestPath = path.join(parityRequestDirectory, 'fulfillment-order-batch-atomicity-orders.graphql');
const splitRequestPath = path.join(parityRequestDirectory, 'fulfillment-order-split-batch-atomicity.graphql');
const mergeRequestPath = path.join(parityRequestDirectory, 'fulfillment-order-merge-batch-atomicity.graphql');

const fulfillmentOrderFields = `#graphql
  fragment FulfillmentOrderBatchAtomicityFields on FulfillmentOrder {
    id
    status
    requestStatus
    updatedAt
    supportedActions {
      action
    }
    assignedLocation {
      name
      location {
        id
        name
      }
    }
    lineItems(first: 10) {
      nodes {
        id
        totalQuantity
        remainingQuantity
        lineItem {
          id
          title
          quantity
          fulfillableQuantity
        }
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderBatchAtomicityOrderCreate(
    $order: OrderCreateOrderInput!
    $options: OrderCreateOptionsInput
  ) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFulfillmentStatus
        fulfillmentOrders(first: 10) {
          nodes {
            ...FulfillmentOrderBatchAtomicityFields
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
  ${fulfillmentOrderFields}
  query FulfillmentOrderBatchAtomicityOrders($firstOrderId: ID!, $secondOrderId: ID!) {
    first: order(id: $firstOrderId) {
      id
      name
      displayFulfillmentStatus
      fulfillmentOrders(first: 10) {
        nodes {
          ...FulfillmentOrderBatchAtomicityFields
        }
      }
    }
    second: order(id: $secondOrderId) {
      id
      name
      displayFulfillmentStatus
      fulfillmentOrders(first: 10) {
        nodes {
          ...FulfillmentOrderBatchAtomicityFields
        }
      }
    }
  }
`;

const splitHydrateQuery = `#graphql
  query ShippingFulfillmentOrderHydrate($id: ID!) {
    fulfillmentOrder(id: $id) {
      id
      status
      requestStatus
      fulfillAt
      fulfillBy
      updatedAt
      supportedActions {
        action
      }
      assignedLocation {
        name
        location {
          id
          name
        }
      }
      fulfillmentHolds {
        id
        handle
        reason
        reasonNotes
        displayReason
        heldByApp {
          id
          title
        }
        heldByRequestingApp
      }
      merchantRequests(first: 10) {
        nodes {
          kind
          message
          requestOptions
        }
      }
      lineItems(first: 20) {
        nodes {
          id
          totalQuantity
          remainingQuantity
          lineItem {
            id
            title
            quantity
            fulfillableQuantity
          }
        }
      }
      order {
        id
        name
        displayFulfillmentStatus
      }
    }
  }
`;

const mergeHydrateQuery = `#graphql
  query ShippingFulfillmentOrdersMergeHydrate($ids: [ID!]!) {
    nodes(ids: $ids) {
      ... on FulfillmentOrder {
        id
        status
        requestStatus
        fulfillAt
        fulfillBy
        updatedAt
        supportedActions {
          action
        }
        assignedLocation {
          name
          location {
            id
            name
          }
        }
        fulfillmentHolds {
          id
          handle
          reason
          reasonNotes
          displayReason
          heldByApp {
            id
            title
          }
          heldByRequestingApp
        }
        merchantRequests(first: 10) {
          nodes {
            kind
            message
            requestOptions
          }
        }
        lineItems(first: 20) {
          nodes {
            id
            totalQuantity
            remainingQuantity
            lineItem {
              id
              title
              quantity
              fulfillableQuantity
            }
          }
        }
        order {
          id
          name
          displayFulfillmentStatus
        }
      }
    }
  }
`;

const splitMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderSplitBatchAtomicity($fulfillmentOrderSplits: [FulfillmentOrderSplitInput!]!) {
    fulfillmentOrderSplit(fulfillmentOrderSplits: $fulfillmentOrderSplits) {
      fulfillmentOrderSplits {
        fulfillmentOrder {
          ...FulfillmentOrderBatchAtomicityFields
        }
        remainingFulfillmentOrder {
          ...FulfillmentOrderBatchAtomicityFields
        }
        replacementFulfillmentOrder {
          ...FulfillmentOrderBatchAtomicityFields
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

const mergeMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderMergeBatchAtomicity(
    $fulfillmentOrderMergeInputs: [FulfillmentOrderMergeInput!]!
  ) {
    fulfillmentOrderMerge(fulfillmentOrderMergeInputs: $fulfillmentOrderMergeInputs) {
      fulfillmentOrderMerges {
        fulfillmentOrder {
          ...FulfillmentOrderBatchAtomicityFields
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

const orderCancelMutation = `#graphql
  mutation FulfillmentOrderBatchAtomicityOrderCancel(
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

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

function formatGeneratedJson(paths: string[]): void {
  const result = spawnSync('corepack', ['pnpm', 'exec', 'oxfmt', ...paths], { stdio: 'inherit' });
  if (result.status !== 0) {
    throw new Error(`Failed to format generated JSON files: ${paths.join(', ')}`);
  }
}

async function capture(query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  return {
    query: trimGraphql(query),
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null ? (value as JsonRecord) : null;
}

function readNodes(value: unknown): JsonRecord[] {
  const record = readObject(value);
  const nodes = record?.['nodes'];
  return Array.isArray(nodes) ? nodes.filter((node): node is JsonRecord => readObject(node) !== null) : [];
}

function fulfillmentOrderFromCreatedOrder(captureResult: GraphqlCapture): JsonRecord {
  const data = readObject(captureResult.response.payload.data);
  const orderCreate = readObject(data?.['orderCreate']);
  const errors = orderCreate?.['userErrors'];
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`Unable to create disposable order: ${JSON.stringify(errors)}`);
  }
  const order = readObject(orderCreate?.['order']);
  const fulfillmentOrder = readNodes(readObject(order?.['fulfillmentOrders']))[0];
  if (!order || typeof order['id'] !== 'string' || !fulfillmentOrder) {
    throw new Error(`Created order has no fulfillment order: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return { ...fulfillmentOrder, orderId: order['id'] };
}

function asCreatedOrder(captureResult: GraphqlCapture): CreatedOrder {
  const fulfillmentOrder = fulfillmentOrderFromCreatedOrder(captureResult);
  const lineItem = readNodes(readObject(fulfillmentOrder['lineItems']))[0];
  if (
    typeof fulfillmentOrder['orderId'] !== 'string' ||
    typeof fulfillmentOrder['id'] !== 'string' ||
    typeof lineItem?.['id'] !== 'string'
  ) {
    throw new Error(`Created fulfillment order is missing identity: ${JSON.stringify(fulfillmentOrder)}`);
  }
  return {
    id: fulfillmentOrder['orderId'],
    fulfillmentOrderId: fulfillmentOrder['id'],
    fulfillmentOrderLineItemId: lineItem['id'],
  };
}

function asSplitPair(split: GraphqlCapture): SplitPair {
  const payload = readObject(readObject(split.response.payload.data)?.['fulfillmentOrderSplit']);
  const errors = payload?.['userErrors'];
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`Unable to split disposable fulfillment order: ${JSON.stringify(errors)}`);
  }
  const result = Array.isArray(payload?.['fulfillmentOrderSplits'])
    ? readObject(payload['fulfillmentOrderSplits'][0])
    : null;
  const primary = readObject(result?.['fulfillmentOrder']);
  const sibling = readObject(result?.['remainingFulfillmentOrder']);
  const siblingLineItem = readNodes(readObject(sibling?.['lineItems']))[0];
  if (
    typeof primary?.['id'] !== 'string' ||
    typeof sibling?.['id'] !== 'string' ||
    typeof siblingLineItem?.['id'] !== 'string'
  ) {
    throw new Error(`Split did not return a mergeable pair: ${JSON.stringify(payload)}`);
  }
  return {
    primaryFulfillmentOrderId: primary['id'],
    siblingFulfillmentOrderId: sibling['id'],
    siblingLineItemId: siblingLineItem['id'],
  };
}

function mutationPayload(captureResult: GraphqlCapture, root: string): JsonRecord {
  const payload = readObject(readObject(captureResult.response.payload.data)?.[root]);
  if (!payload) throw new Error(`${root} did not return a payload: ${JSON.stringify(captureResult.response.payload)}`);
  const errors = payload['userErrors'];
  if (!Array.isArray(errors) || errors.length === 0) {
    throw new Error(`${root} did not return the expected batch error: ${JSON.stringify(payload)}`);
  }
  return payload;
}

async function createTrackedOrder(label: string): Promise<{ order: CreatedOrder; create: GraphqlCapture }> {
  const stamp = new Date()
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14);
  const create = await capture(orderCreateMutation, {
    order: {
      email: `${captureId}-${label}-${stamp}@example.com`,
      note: `Fulfillment order batch atomicity ${label} ${stamp}`,
      tags: [captureId, label],
      test: true,
      currency: 'USD',
      shippingAddress: {
        firstName: 'Batch',
        lastName: 'Atomicity',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          variantId: physicalVariantId,
          title: `Fulfillment order batch atomicity ${label} ${stamp}`,
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
  });
  return { order: asCreatedOrder(create), create };
}

async function splitOrder(order: CreatedOrder): Promise<{ split: GraphqlCapture; pair: SplitPair }> {
  const split = await capture(splitMutation, {
    fulfillmentOrderSplits: [
      {
        fulfillmentOrderId: order.fulfillmentOrderId,
        fulfillmentOrderLineItems: [{ id: order.fulfillmentOrderLineItemId, quantity: 1 }],
      },
    ],
  });
  return { split, pair: asSplitPair(split) };
}

async function readOrders(first: CreatedOrder, second: CreatedOrder): Promise<GraphqlCapture> {
  return capture(orderReadQuery, { firstOrderId: first.id, secondOrderId: second.id });
}

async function cleanupOrder(order: CreatedOrder): Promise<GraphqlCapture> {
  return capture(orderCancelMutation, {
    orderId: order.id,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
}

function recordedUpstreamCall(captured: GraphqlCapture): JsonRecord {
  const operationName = /\b(?:query|mutation)\s+([_A-Za-z][_0-9A-Za-z]*)/u.exec(captured.query)?.[1] ?? 'Unknown';
  return {
    method: 'POST',
    apiSurface: 'admin',
    apiVersion,
    path: `/admin/api/${apiVersion}/graphql.json`,
    operationName,
    query: captured.query,
    variables: captured.variables,
    response: {
      status: captured.response.status,
      body: captured.response.payload,
    },
  };
}

function paritySpec(root: 'split' | 'merge'): JsonRecord {
  const operationName = root === 'split' ? 'fulfillmentOrderSplit' : 'fulfillmentOrderMerge';
  const resultField = root === 'split' ? 'fulfillmentOrderSplits' : 'fulfillmentOrderMerges';
  const mutationDocument = root === 'split' ? splitRequestPath : mergeRequestPath;
  return {
    scenarioId: `fulfillment-order-${root}-batch-atomicity`,
    operationNames: [operationName],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity', 'validation-atomicity', 'downstream-read-parity'],
    liveCaptureFiles: [fixtureReference],
    runtimeTestFiles: ['tests/graphql_routes/platform.rs'],
    proxyRequest: {
      documentPath: orderReadRequestPath.replaceAll(path.sep, '/'),
      variablesCapturePath: `$.${root}.before.variables`,
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: `${root}-orders-before-failure`,
          capturePath: `$.${root}.before.response.payload.data`,
          proxyPath: '$.data',
        },
        {
          name: `${root}-valid-first-invalid-second-rejected`,
          capturePath: `$.${root}.failure.response.payload.data.${operationName}`,
          proxyPath: `$.data.${operationName}`,
          proxyRequest: {
            documentPath: mutationDocument.replaceAll(path.sep, '/'),
            variablesCapturePath: `$.${root}.failure.variables`,
            apiVersion,
          },
        },
        {
          name: `${root}-orders-unchanged-after-failure`,
          capturePath: `$.${root}.after.response.payload.data`,
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: orderReadRequestPath.replaceAll(path.sep, '/'),
            variablesCapturePath: `$.${root}.after.variables`,
            apiVersion,
          },
        },
      ],
    },
    notes: `Live Admin GraphQL evidence that a valid first ${operationName} input followed by an invalid second input returns ${resultField}: null and leaves both owning orders unchanged. The replay hydrates both orders through the public order query and keeps the supported mutation local.`,
  };
}

const startedAt = new Date().toISOString();
const createdOrders: CreatedOrder[] = [];
const cleanup: GraphqlCapture[] = [];
const cleanedOrderIds = new Set<string>();

async function trackOrder(label: string): Promise<{ order: CreatedOrder; create: GraphqlCapture }> {
  const created = await createTrackedOrder(label);
  createdOrders.push(created.order);
  return created;
}

async function cleanupCreatedOrders(): Promise<void> {
  for (const order of createdOrders) {
    if (cleanedOrderIds.has(order.id)) continue;
    cleanedOrderIds.add(order.id);
    try {
      cleanup.push(await cleanupOrder(order));
    } catch (error) {
      console.error(`Failed to clean up disposable order ${order.id}:`, error);
    }
  }
}

async function main(): Promise<void> {
  try {
    const splitFirst = await trackOrder('split-first');
    const splitSecond = await trackOrder('split-second');
    const splitBefore = await readOrders(splitFirst.order, splitSecond.order);
    const splitFirstHydrate = await capture(splitHydrateQuery, { id: splitFirst.order.fulfillmentOrderId });
    const splitSecondHydrate = await capture(splitHydrateQuery, { id: splitSecond.order.fulfillmentOrderId });
    const splitFailure = await capture(splitMutation, {
      fulfillmentOrderSplits: [
        {
          fulfillmentOrderId: splitFirst.order.fulfillmentOrderId,
          fulfillmentOrderLineItems: [{ id: splitFirst.order.fulfillmentOrderLineItemId, quantity: 1 }],
        },
        {
          fulfillmentOrderId: splitSecond.order.fulfillmentOrderId,
          fulfillmentOrderLineItems: [{ id: splitSecond.order.fulfillmentOrderLineItemId, quantity: 3 }],
        },
      ],
    });
    const splitPayload = mutationPayload(splitFailure, 'fulfillmentOrderSplit');
    if (splitPayload['fulfillmentOrderSplits'] !== null) {
      throw new Error(`Rejected split batch returned results: ${JSON.stringify(splitPayload)}`);
    }
    const splitAfter = await readOrders(splitFirst.order, splitSecond.order);
    deepStrictEqual(splitAfter.response.payload.data, splitBefore.response.payload.data);

    const mergeFirst = await trackOrder('merge-first');
    const mergeSecond = await trackOrder('merge-second');
    const mergeFirstSplit = await splitOrder(mergeFirst.order);
    const mergeSecondSplit = await splitOrder(mergeSecond.order);
    const mergeBefore = await readOrders(mergeFirst.order, mergeSecond.order);
    const mergeHydrate = await capture(mergeHydrateQuery, {
      ids: [
        mergeFirstSplit.pair.primaryFulfillmentOrderId,
        mergeFirstSplit.pair.siblingFulfillmentOrderId,
        mergeSecondSplit.pair.primaryFulfillmentOrderId,
        mergeSecondSplit.pair.siblingFulfillmentOrderId,
      ],
    });
    const mergeFailure = await capture(mergeMutation, {
      fulfillmentOrderMergeInputs: [
        {
          mergeIntents: [
            { fulfillmentOrderId: mergeFirstSplit.pair.primaryFulfillmentOrderId },
            { fulfillmentOrderId: mergeFirstSplit.pair.siblingFulfillmentOrderId },
          ],
        },
        {
          mergeIntents: [
            { fulfillmentOrderId: mergeSecondSplit.pair.primaryFulfillmentOrderId },
            {
              fulfillmentOrderId: mergeSecondSplit.pair.siblingFulfillmentOrderId,
              fulfillmentOrderLineItems: [
                {
                  id: mergeSecondSplit.pair.siblingLineItemId,
                  quantity: 2,
                },
              ],
            },
          ],
        },
      ],
    });
    const mergePayload = mutationPayload(mergeFailure, 'fulfillmentOrderMerge');
    if (mergePayload['fulfillmentOrderMerges'] !== null) {
      throw new Error(`Rejected merge batch returned results: ${JSON.stringify(mergePayload)}`);
    }
    const mergeAfter = await readOrders(mergeFirst.order, mergeSecond.order);
    deepStrictEqual(mergeAfter.response.payload.data, mergeBefore.response.payload.data);

    await cleanupCreatedOrders();

    const output = {
      metadata: {
        source: 'live-shopify',
        capturedAt: new Date().toISOString(),
        startedAt,
        storeDomain,
        apiVersion,
        scopedRoots: ['fulfillmentOrderSplit', 'fulfillmentOrderMerge'],
        createdOrders,
      },
      split: {
        firstCreate: splitFirst.create,
        secondCreate: splitSecond.create,
        before: splitBefore,
        firstHydrate: splitFirstHydrate,
        secondHydrate: splitSecondHydrate,
        failure: splitFailure,
        after: splitAfter,
      },
      merge: {
        firstCreate: mergeFirst.create,
        secondCreate: mergeSecond.create,
        firstSetupSplit: mergeFirstSplit.split,
        secondSetupSplit: mergeSecondSplit.split,
        before: mergeBefore,
        hydrate: mergeHydrate,
        failure: mergeFailure,
        after: mergeAfter,
      },
      cleanup,
      upstreamCalls: [
        recordedUpstreamCall(splitBefore),
        recordedUpstreamCall(splitFirstHydrate),
        recordedUpstreamCall(splitSecondHydrate),
        recordedUpstreamCall(splitAfter),
        recordedUpstreamCall(mergeBefore),
        recordedUpstreamCall(mergeHydrate),
        recordedUpstreamCall(mergeAfter),
      ],
    };

    await Promise.all([
      mkdir(fixtureDirectory, { recursive: true }),
      mkdir(paritySpecDirectory, { recursive: true }),
      mkdir(parityRequestDirectory, { recursive: true }),
    ]);
    await Promise.all([
      writeFile(fixturePath, `${JSON.stringify(output, null, 2)}\n`, 'utf8'),
      writeFile(splitSpecPath, `${JSON.stringify(paritySpec('split'), null, 2)}\n`, 'utf8'),
      writeFile(mergeSpecPath, `${JSON.stringify(paritySpec('merge'), null, 2)}\n`, 'utf8'),
      writeFile(orderReadRequestPath, `${trimGraphql(orderReadQuery)}\n`, 'utf8'),
      writeFile(splitRequestPath, `${trimGraphql(splitMutation)}\n`, 'utf8'),
      writeFile(mergeRequestPath, `${trimGraphql(mergeMutation)}\n`, 'utf8'),
    ]);
    formatGeneratedJson([fixturePath, splitSpecPath, mergeSpecPath]);

    console.log(`Captured fulfillment-order batch atomicity fixture: ${fixturePath}`);
  } finally {
    await cleanupCreatedOrders();
  }
}

await main();
