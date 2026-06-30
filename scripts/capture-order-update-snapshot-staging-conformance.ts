/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CaptureStep = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
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
const fixturePath = path.join(fixtureDir, 'orderUpdate-snapshot-staging.json');
const specPath = path.join('config', 'parity-specs', 'orders', 'orderUpdate-snapshot-staging.json');
const createRequestPath = path.join(
  'config',
  'parity-requests',
  'orders',
  'orderUpdate-snapshot-staging-create.graphql',
);
const updateRequestPath = path.join('config', 'parity-requests', 'orders', 'orderUpdate-snapshot-staging.graphql');
const readRequestPath = path.join('config', 'parity-requests', 'orders', 'orderUpdate-snapshot-staging-read.graphql');

const orderCreateDocument = `#graphql
  mutation OrderUpdateSnapshotStagingCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        updatedAt
        email
        phone
        poNumber
        note
        tags
        customer {
          id
          email
          displayName
        }
        customAttributes {
          key
          value
        }
        shippingAddress {
          firstName
          lastName
          address1
          address2
          company
          city
          province
          provinceCode
          country
          countryCodeV2
          zip
          phone
        }
        lineItems(first: 5) {
          nodes {
            id
            title
            quantity
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

const orderUpdateDocument = `#graphql
  mutation OrderUpdateSnapshotStaging($input: OrderInput!) {
    orderUpdate(input: $input) {
      order {
        id
        name
        updatedAt
        email
        phone
        poNumber
        note
        tags
        customer {
          id
          email
          displayName
        }
        customAttributes {
          key
          value
        }
        shippingAddress {
          firstName
          lastName
          address1
          address2
          company
          city
          province
          provinceCode
          country
          countryCodeV2
          zip
          phone
        }
        gift: metafield(namespace: "custom", key: "gift") {
          id
          namespace
          key
          type
          value
        }
        metafields(first: 10) {
          nodes {
            id
            namespace
            key
            type
            value
          }
        }
        lineItems(first: 5) {
          nodes {
            id
            title
            quantity
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

const downstreamReadDocument = `#graphql
  query OrderUpdateSnapshotStagingRead($id: ID!, $query: String!) {
    byId: order(id: $id) {
      id
      name
      updatedAt
      email
      phone
      poNumber
      note
      tags
      customer {
        email
      }
      customAttributes {
        key
        value
      }
      shippingAddress {
        provinceCode
        countryCodeV2
        phone
      }
      gift: metafield(namespace: "custom", key: "gift") {
        id
        namespace
        key
        type
        value
      }
      metafields(first: 10) {
        nodes {
          id
          namespace
          key
          type
          value
        }
      }
      lineItems(first: 5) {
        nodes {
          id
          title
          quantity
        }
      }
    }
    orders(first: 5, query: $query) {
      nodes {
        id
        email
        note
        tags
      }
    }
    ordersCount(query: $query) {
      count
      precision
    }
  }
`;

const orderDeleteDocument = `#graphql
  mutation OrderUpdateSnapshotStagingCleanupDelete($orderId: ID!) {
    orderDelete(orderId: $orderId) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
}

async function writeText(filePath: string, payload: string): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${payload.trim()}\n`, 'utf8');
}

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readRecord(value: unknown, key: string): JsonRecord | null {
  return asRecord(asRecord(value)?.[key]);
}

function readArray(value: unknown, key: string): unknown[] {
  const fieldValue = asRecord(value)?.[key];
  return Array.isArray(fieldValue) ? fieldValue : [];
}

function readString(value: unknown, key: string): string | null {
  const fieldValue = asRecord(value)?.[key];
  return typeof fieldValue === 'string' && fieldValue.length > 0 ? fieldValue : null;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult<JsonRecord>, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

async function capture(query: string, variables: JsonRecord, context: string): Promise<CaptureStep> {
  const response = await runGraphqlRequest<JsonRecord>(trimGraphql(query), variables);
  assertNoTopLevelErrors(response, context);
  return { query: trimGraphql(query), variables, response };
}

async function captureDownstreamReadWithIndexedSearch(orderId: string, updatedEmail: string): Promise<CaptureStep> {
  const variables = { id: orderId, query: `email:${updatedEmail}` };
  let last: CaptureStep | null = null;
  for (let attempt = 0; attempt < 10; attempt += 1) {
    last = await capture(downstreamReadDocument, variables, `orderUpdate downstream read attempt ${attempt + 1}`);
    const data = asRecord(last.response.payload.data);
    const byId = readRecord(data, 'byId');
    const orderNodes = readArray(readRecord(data, 'orders'), 'nodes');
    const orderCount = readRecord(data, 'ordersCount');
    if (
      readString(byId, 'email') === updatedEmail &&
      readString(asRecord(orderNodes[0]), 'email') === updatedEmail &&
      orderCount?.['count'] === 1
    ) {
      return last;
    }
    await sleep(500);
  }
  throw new Error(
    `Downstream reads did not expose updated email after retry: ${JSON.stringify(last?.response.payload)}`,
  );
}

function payloadFor(step: CaptureStep, rootName: string): JsonRecord {
  const payload = readRecord(step.response.payload.data, rootName);
  if (!payload) {
    throw new Error(`Missing ${rootName} payload: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return payload;
}

function assertEmptyUserErrors(step: CaptureStep, rootName: string): void {
  const userErrors = readArray(payloadFor(step, rootName), 'userErrors');
  if (userErrors.length > 0) {
    throw new Error(`${rootName} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function orderFrom(step: CaptureStep, rootName: string): JsonRecord {
  const order = readRecord(payloadFor(step, rootName), 'order');
  if (!order) {
    throw new Error(`Missing ${rootName}.order: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return order;
}

function orderIdFrom(step: CaptureStep, rootName: string): string {
  const orderId = readString(orderFrom(step, rootName), 'id');
  if (!orderId) {
    throw new Error(`Missing ${rootName}.order.id: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return orderId;
}

function orderCreateVariables(stamp: number): JsonRecord {
  return {
    order: {
      email: `order-update-snapshot-before-${stamp}@example.com`,
      note: 'order update snapshot baseline',
      tags: ['snapshot-baseline'],
      test: true,
      currency: 'USD',
      lineItems: [
        {
          title: `Order update snapshot item ${stamp}`,
          quantity: 2,
          priceSet: {
            shopMoney: {
              amount: '12.00',
              currencyCode: 'USD',
            },
          },
          requiresShipping: false,
          taxable: false,
          sku: `order-update-snapshot-${stamp}`,
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
              amount: '24.00',
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

function orderUpdateVariables(orderId: string, stamp: number): JsonRecord {
  return {
    input: {
      id: orderId,
      email: `order-update-snapshot-after-${stamp}@example.com`,
      phone: '+16135551111',
      poNumber: `PO-SNAPSHOT-${stamp}`,
      note: 'order update snapshot after',
      tags: ['snapshot-after', 'vip'],
      customAttributes: [{ key: 'source', value: 'snapshot-staging' }],
      shippingAddress: {
        firstName: 'Ada',
        lastName: 'Lovelace',
        address1: '190 MacLaren',
        address2: 'Suite 200',
        company: 'Analytical Engines Ltd',
        city: 'Sudbury',
        province: 'Ontario',
        provinceCode: 'ON',
        country: 'Canada',
        countryCode: 'CA',
        zip: 'K2P0V6',
        phone: '+16135552222',
      },
      metafields: [
        {
          namespace: 'custom',
          key: 'gift',
          type: 'single_line_text_field',
          value: 'wrapped',
        },
      ],
    },
  };
}

function orderUpdateSpecVariables(): JsonRecord {
  const inputPath = '$.operations.update.variables.input';
  return {
    input: {
      id: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
      email: { fromCapturePath: `${inputPath}.email` },
      phone: { fromCapturePath: `${inputPath}.phone` },
      poNumber: { fromCapturePath: `${inputPath}.poNumber` },
      note: { fromCapturePath: `${inputPath}.note` },
      tags: { fromCapturePath: `${inputPath}.tags` },
      customAttributes: { fromCapturePath: `${inputPath}.customAttributes` },
      shippingAddress: { fromCapturePath: `${inputPath}.shippingAddress` },
      metafields: { fromCapturePath: `${inputPath}.metafields` },
    },
  };
}

function selectedStableMutationPaths(): string[] {
  return [
    '$.order.email',
    '$.order.phone',
    '$.order.poNumber',
    '$.order.note',
    '$.order.tags',
    '$.order.customer.email',
    '$.order.customAttributes',
    '$.order.shippingAddress.firstName',
    '$.order.shippingAddress.lastName',
    '$.order.shippingAddress.address1',
    '$.order.shippingAddress.address2',
    '$.order.shippingAddress.company',
    '$.order.shippingAddress.city',
    '$.order.shippingAddress.province',
    '$.order.shippingAddress.provinceCode',
    '$.order.shippingAddress.country',
    '$.order.shippingAddress.countryCodeV2',
    '$.order.shippingAddress.zip',
    '$.order.shippingAddress.phone',
    '$.order.gift.namespace',
    '$.order.gift.key',
    '$.order.gift.type',
    '$.order.gift.value',
    '$.order.metafields.nodes[0].namespace',
    '$.order.metafields.nodes[0].key',
    '$.order.metafields.nodes[0].type',
    '$.order.metafields.nodes[0].value',
    '$.order.lineItems.nodes[0].title',
    '$.order.lineItems.nodes[0].quantity',
    '$.userErrors',
  ];
}

function selectedStableReadPaths(): string[] {
  return [
    '$.byId.email',
    '$.byId.phone',
    '$.byId.poNumber',
    '$.byId.note',
    '$.byId.tags',
    '$.byId.customer.email',
    '$.byId.customAttributes',
    '$.byId.shippingAddress.provinceCode',
    '$.byId.shippingAddress.countryCodeV2',
    '$.byId.shippingAddress.phone',
    '$.byId.gift.namespace',
    '$.byId.gift.key',
    '$.byId.gift.type',
    '$.byId.gift.value',
    '$.byId.metafields.nodes[0].namespace',
    '$.byId.metafields.nodes[0].key',
    '$.byId.metafields.nodes[0].type',
    '$.byId.metafields.nodes[0].value',
    '$.byId.lineItems.nodes[0].title',
    '$.byId.lineItems.nodes[0].quantity',
    '$.orders.nodes[0].email',
    '$.orders.nodes[0].note',
    '$.orders.nodes[0].tags',
    '$.ordersCount',
  ];
}

async function cleanupOrder(orderId: string): Promise<CaptureStep> {
  return capture(orderDeleteDocument, { orderId }, 'orderDelete cleanup');
}

const stamp = Date.now();
const create = await capture(orderCreateDocument, orderCreateVariables(stamp), 'orderCreate setup');
assertEmptyUserErrors(create, 'orderCreate');
const orderId = orderIdFrom(create, 'orderCreate');

const update = await capture(orderUpdateDocument, orderUpdateVariables(orderId, stamp), 'orderUpdate happy path');
assertEmptyUserErrors(update, 'orderUpdate');
const updatedEmail = readString(orderFrom(update, 'orderUpdate'), 'email');
if (!updatedEmail) {
  throw new Error(`orderUpdate response did not include updated email: ${JSON.stringify(update.response.payload)}`);
}

const downstreamRead = await captureDownstreamReadWithIndexedSearch(orderId, updatedEmail);

let cleanup: CaptureStep | null = null;
try {
  cleanup = await cleanupOrder(orderId);
} catch (error) {
  console.error(`Cleanup orderDelete failed for ${orderId}: ${(error as Error).message}`);
}

await writeText(createRequestPath, trimGraphql(orderCreateDocument));
await writeText(updateRequestPath, trimGraphql(orderUpdateDocument));
await writeText(readRequestPath, trimGraphql(downstreamReadDocument));
await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  operations: {
    create,
    update,
    downstreamRead,
  },
  cleanup,
  upstreamCalls: [],
});
await writeJson(specPath, {
  scenarioId: 'orderUpdate-snapshot-staging',
  operationNames: ['orderCreate', 'orderUpdate', 'order', 'orders', 'ordersCount'],
  scenarioStatus: 'captured',
  assertionKinds: [
    'payload-shape',
    'runtime-staging',
    'read-after-write',
    'downstream-read-parity',
    'mutation-log-raw-body',
    'no-upstream-passthrough',
  ],
  liveCaptureFiles: [fixturePath],
  runtimeTestFiles: ['tests/graphql_routes/orders.rs'],
  proxyRequest: {
    documentPath: createRequestPath,
    variablesCapturePath: '$.operations.create.variables',
    apiVersion,
  },
  comparisonMode: 'captured-vs-proxy-request',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [],
    targets: [
      {
        name: 'create-baseline-order',
        capturePath: '$.operations.create.response.payload.data.orderCreate',
        proxyPath: '$.data.orderCreate',
        selectedPaths: [
          '$.order.email',
          '$.order.note',
          '$.order.tags',
          '$.order.lineItems.nodes[0].title',
          '$.order.lineItems.nodes[0].quantity',
          '$.userErrors',
        ],
      },
      {
        name: 'order-update-stages-simple-fields',
        capturePath: '$.operations.update.response.payload.data.orderUpdate',
        proxyPath: '$.data.orderUpdate',
        selectedPaths: selectedStableMutationPaths(),
        proxyRequest: {
          documentPath: updateRequestPath,
          variables: orderUpdateSpecVariables(),
          apiVersion,
        },
      },
      {
        name: 'downstream-order-reads-reflect-update',
        capturePath: '$.operations.downstreamRead.response.payload.data',
        proxyPath: '$.data',
        selectedPaths: selectedStableReadPaths(),
        proxyRequest: {
          documentPath: readRequestPath,
          variables: {
            id: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
            query: { fromCapturePath: '$.operations.downstreamRead.variables.query' },
          },
          apiVersion,
        },
      },
    ],
  },
  notes:
    'Live Shopify capture for public orderCreate -> orderUpdate -> downstream order/order connection/ordersCount behavior. The capture script polls the post-update order email search because Shopify search indexing can lag immediately after orderUpdate. Shopify updates Order.email but preserves the nested Customer.email from the original order customer. The proxy replay uses the same public GraphQL request surface in snapshot-style local staging without runtime Shopify writes; runtime tests cover raw mutation-log retention and synthetic identity/line-item preservation.',
});

console.log(JSON.stringify({ ok: true, fixturePath, specPath, orderId, cleanup: cleanup !== null }, null, 2));
