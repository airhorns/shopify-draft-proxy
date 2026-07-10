/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
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

const scenarioId = 'b2b-company-location-order-aggregates';
const expectedApiVersion = '2025-01';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: expectedApiVersion,
  exitOnMissing: true,
});

if (apiVersion !== expectedApiVersion) {
  throw new Error(`${scenarioId} requires SHOPIFY_CONFORMANCE_API_VERSION=${expectedApiVersion}, got ${apiVersion}`);
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const requestDir = path.join('config', 'parity-requests', 'b2b');
const specPath = path.join('config', 'parity-specs', 'b2b', `${scenarioId}.json`);
const companyCreateRequestPath = path.join(requestDir, `${scenarioId}-company-create.graphql`);
const draftOrderCreateRequestPath = path.join(requestDir, `${scenarioId}-draft-order-create.graphql`);
const draftOrderCompleteRequestPath = path.join(requestDir, `${scenarioId}-draft-order-complete.graphql`);
const catalogCreateRequestPath = path.join(requestDir, `${scenarioId}-catalog-create.graphql`);
const aggregateReadRequestPath = path.join(requestDir, `${scenarioId}-read.graphql`);
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b');
const fixturePath = path.join(fixtureDir, `${scenarioId}.json`);

const companyCreateDocument = `#graphql
  mutation B2BCompanyLocationOrderAggregatesCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        mainContact {
          id
          customer {
            id
          }
        }
        locations(first: 5) {
          nodes {
            id
            name
          }
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

const draftOrderCreateDocument = `#graphql
  mutation B2BCompanyLocationOrderAggregatesDraftOrderCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        status
        totalPriceSet {
          shopMoney {
            amount
            currencyCode
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

const draftOrderCompleteDocument = `#graphql
  mutation B2BCompanyLocationOrderAggregatesDraftOrderComplete($id: ID!, $paymentPending: Boolean!) {
    draftOrderComplete(id: $id, paymentPending: $paymentPending) {
      draftOrder {
        id
        status
        order {
          id
          name
          currentTotalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          totalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          displayFinancialStatus
          totalReceivedSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          purchasingEntity {
            ... on PurchasingCompany {
              company {
                id
              }
              contact {
                id
              }
              location {
                id
              }
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

const draftOrderDeleteDocument = `#graphql
  mutation B2BCompanyLocationOrderAggregatesDraftOrderDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const catalogCreateDocument = `#graphql
  mutation B2BCompanyLocationOrderAggregatesCatalogCreate($input: CatalogCreateInput!) {
    catalogCreate(input: $input) {
      catalog {
        __typename
        id
        title
        status
        ... on CompanyLocationCatalog {
          companyLocations(first: 5) {
            nodes {
              id
              name
            }
          }
        }
      }
      userErrors {
        __typename
        field
        message
        code
      }
    }
  }
`;

const aggregateReadDocument = `#graphql
  query B2BCompanyLocationOrderAggregatesRead($companyId: ID!, $locationId: ID!) {
    company(id: $companyId) {
      id
      totalSpent {
        amount
        currencyCode
      }
      spend: totalSpent {
        value: amount
        currencyCode
      }
      ordersCount {
        count
        precision
      }
      orderSummary: ordersCount {
        total: count
        precision
      }
      orders(first: 1) {
        nodes {
          id
          name
          currentTotalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
        }
      }
      draftOrders(first: 1) {
        nodes {
          id
          name
          status
          totalPriceSet {
            shopMoney {
              amount
              currencyCode
            }
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
        }
      }
      lifetimeDuration
    }
    companyLocation(id: $locationId) {
      id
      totalSpent {
        amount
        currencyCode
      }
      locationSpend: totalSpent {
        value: amount
        currencyCode
      }
      currency
      ordersCount {
        count
        precision
      }
      orderSummary: ordersCount {
        total: count
        precision
      }
      orders(first: 1) {
        nodes {
          id
          name
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
        }
      }
      draftOrders(first: 1) {
        nodes {
          id
          name
          status
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
        }
      }
      catalogs(first: 5) {
        nodes {
          __typename
          id
          title
        }
      }
    }
    companyNode: node(id: $companyId) {
      __typename
      ... on Company {
        totalSpent {
          amount
          currencyCode
        }
        ordersCount {
          count
          precision
        }
        lifetimeDuration
      }
    }
    locationNode: node(id: $locationId) {
      __typename
      ... on CompanyLocation {
        totalSpent {
          amount
          currencyCode
        }
        currency
        ordersCount {
          count
          precision
        }
        catalogs(first: 5) {
          nodes {
            __typename
            id
            title
          }
        }
      }
    }
  }
`;

const orderCancelDocument = `#graphql
  mutation B2BCompanyLocationOrderAggregatesOrderCancel(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
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

const orderDeleteDocument = `#graphql
  mutation B2BCompanyLocationOrderAggregatesOrderDelete($orderId: ID!) {
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

const catalogDeleteDocument = `#graphql
  mutation B2BCompanyLocationOrderAggregatesCatalogDelete($id: ID!) {
    catalogDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BCompanyLocationOrderAggregatesCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function writeText(filePath: string, payload: string): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${payload.trim()}\n`, 'utf8');
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
}

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      const index = Number(segment);
      if (!Number.isInteger(index) || index < 0) {
        return undefined;
      }
      current = current[index];
      continue;
    }

    const record = asRecord(current);
    if (!record) {
      return undefined;
    }
    current = record[segment];
  }
  return current;
}

function readStringAtPath(value: unknown, pathSegments: string[], label: string): string {
  const pathValue = readPath(value, pathSegments);
  if (typeof pathValue !== 'string' || pathValue.length === 0) {
    throw new Error(`${label} did not return a string at ${pathSegments.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return pathValue;
}

function readArrayAtPath(value: unknown, pathSegments: string[]): unknown[] {
  const pathValue = readPath(value, pathSegments);
  return Array.isArray(pathValue) ? pathValue : [];
}

function userErrors(result: ConformanceGraphqlResult<JsonRecord>, root: string): unknown[] {
  return readArrayAtPath(result.payload, ['data', root, 'userErrors']);
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult<JsonRecord>, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertSuccessful(result: ConformanceGraphqlResult<JsonRecord>, root: string, context: string): void {
  assertNoTopLevelErrors(result, context);
  const errors = userErrors(result, root);
  if (errors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

async function capture(query: string, variables: JsonRecord, context: string): Promise<CaptureStep> {
  const trimmed = trimGraphql(query);
  const response = await runGraphqlRequest<JsonRecord>(trimmed, variables);
  assertNoTopLevelErrors(response, context);
  return { query: trimmed, variables, response };
}

async function captureRequired(
  query: string,
  variables: JsonRecord,
  root: string,
  context: string,
): Promise<CaptureStep> {
  const step = await capture(query, variables, context);
  assertSuccessful(step.response, root, context);
  return step;
}

async function sleep(milliseconds: number): Promise<void> {
  await new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}

function companyVariables(stamp: string): JsonRecord {
  return {
    input: {
      company: {
        name: `B2B aggregate buyer ${stamp}`,
        externalId: `b2b-aggregate-buyer-${stamp}`,
        note: `B2B aggregate buyer ${stamp}`,
      },
      companyContact: {
        firstName: 'Aggregate',
        lastName: 'Buyer',
        email: `b2b-aggregate-buyer-${stamp}@example.com`,
        title: 'Buyer',
      },
      companyLocation: {
        name: `B2B aggregate buyer ${stamp} HQ`,
        phone: '+16135550145',
        shippingAddress: {
          address1: '145 Aggregate Way',
          city: 'Ottawa',
          countryCode: 'CA',
        },
      },
    },
  };
}

function draftOrderVariables(
  stamp: string,
  companyId: string,
  companyContactId: string,
  companyLocationId: string,
): JsonRecord {
  return {
    input: {
      purchasingEntity: {
        purchasingCompany: {
          companyId,
          companyContactId,
          companyLocationId,
        },
      },
      email: `b2b-aggregate-draft-${stamp}@example.com`,
      note: `B2B company/location aggregate order ${stamp}`,
      tags: ['b2b-company-location-order-aggregates', `b2b-aggregate-${stamp}`],
      visibleToCustomer: false,
      lineItems: [
        {
          title: `B2B aggregate item ${stamp}`,
          quantity: 1,
          originalUnitPrice: '25.00',
          requiresShipping: false,
          taxable: false,
        },
      ],
    },
  };
}

function draftOrderCompleteVariables(draftOrderId: string): JsonRecord {
  return {
    id: draftOrderId,
    paymentPending: false,
  };
}

function catalogVariables(stamp: string, companyLocationId: string): JsonRecord {
  return {
    input: {
      title: `B2B aggregate catalog ${stamp}`,
      status: 'ACTIVE',
      context: {
        companyLocationIds: [companyLocationId],
      },
    },
  };
}

function aggregateReadVariables(companyId: string, locationId: string): JsonRecord {
  return { companyId, locationId };
}

function assertAggregateRead(step: CaptureStep, catalogTitle: string): void {
  const data = asRecord(step.response.payload.data);
  const company = asRecord(data?.['company']);
  const location = asRecord(data?.['companyLocation']);
  const companyNode = asRecord(data?.['companyNode']);
  const locationNode = asRecord(data?.['locationNode']);
  const companyOrders = readArrayAtPath(company, ['orders', 'nodes']);
  const companyDraftOrders = readArrayAtPath(company, ['draftOrders', 'nodes']);
  const locationOrders = readArrayAtPath(location, ['orders', 'nodes']);
  const locationDraftOrders = readArrayAtPath(location, ['draftOrders', 'nodes']);
  const catalogNodes = readArrayAtPath(location, ['catalogs', 'nodes']);
  const nodeCatalogNodes = readArrayAtPath(locationNode, ['catalogs', 'nodes']);
  const hasCatalog = catalogNodes.some((node) => asRecord(node)?.['title'] === catalogTitle);
  const nodeHasCatalog = nodeCatalogNodes.some((node) => asRecord(node)?.['title'] === catalogTitle);

  const aggregateShapeMatches =
    readPath(company, ['totalSpent', 'amount']) === '25.0' &&
    readPath(company, ['totalSpent', 'currencyCode']) === 'CAD' &&
    readPath(company, ['spend', 'value']) === '25.0' &&
    readPath(company, ['ordersCount', 'count']) === 1 &&
    readPath(company, ['orderSummary', 'total']) === 1 &&
    companyOrders.length === 1 &&
    readPath(companyOrders, ['0', 'currentTotalPriceSet', 'shopMoney', 'amount']) === '25.0' &&
    readPath(companyOrders, ['0', 'currentTotalPriceSet', 'shopMoney', 'currencyCode']) === 'CAD' &&
    readPath(company, ['orders', 'pageInfo', 'hasPreviousPage']) === false &&
    companyDraftOrders.length === 1 &&
    readPath(companyDraftOrders, ['0', 'status']) === 'COMPLETED' &&
    readPath(companyDraftOrders, ['0', 'totalPriceSet', 'shopMoney', 'amount']) === '25.0' &&
    readPath(companyDraftOrders, ['0', 'totalPriceSet', 'shopMoney', 'currencyCode']) === 'CAD' &&
    readPath(company, ['draftOrders', 'pageInfo', 'hasPreviousPage']) === false &&
    readPath(location, ['totalSpent', 'amount']) === '25.0' &&
    readPath(location, ['totalSpent', 'currencyCode']) === 'CAD' &&
    readPath(location, ['locationSpend', 'value']) === '25.0' &&
    readPath(location, ['currency']) === 'CAD' &&
    readPath(location, ['ordersCount', 'count']) === 1 &&
    readPath(location, ['orderSummary', 'total']) === 1 &&
    locationOrders.length === 1 &&
    locationDraftOrders.length === 1 &&
    readPath(locationDraftOrders, ['0', 'status']) === 'COMPLETED' &&
    hasCatalog &&
    readPath(companyNode, ['totalSpent', 'amount']) === '25.0' &&
    readPath(companyNode, ['ordersCount', 'count']) === 1 &&
    readPath(locationNode, ['totalSpent', 'amount']) === '25.0' &&
    readPath(locationNode, ['currency']) === 'CAD' &&
    readPath(locationNode, ['ordersCount', 'count']) === 1 &&
    nodeHasCatalog;

  if (!aggregateShapeMatches) {
    throw new Error(`aggregate read did not return indexed order/catalog values: ${JSON.stringify(data, null, 2)}`);
  }
}

async function captureAggregateReadWithRetry(variables: JsonRecord, catalogTitle: string): Promise<CaptureStep> {
  let latest: CaptureStep | null = null;
  for (let attempt = 1; attempt <= 12; attempt += 1) {
    latest = await capture(aggregateReadDocument, variables, `aggregate read attempt ${attempt}`);
    try {
      assertAggregateRead(latest, catalogTitle);
      if (attempt > 1) {
        console.log(`B2B aggregate read indexed after ${attempt} attempts`);
      }
      return latest;
    } catch (error) {
      if (attempt === 12) {
        throw error;
      }
      await sleep(2_000);
    }
  }
  throw new Error(`aggregate read retry exhausted: ${JSON.stringify(latest?.response.payload, null, 2)}`);
}

async function cleanupCapture(query: string, variables: JsonRecord, context: string): Promise<CaptureStep> {
  return capture(query, variables, context);
}

function specPayload(): JsonRecord {
  return {
    scenarioId,
    operationNames: [
      'companyCreate',
      'draftOrderCreate',
      'draftOrderComplete',
      'catalogCreate',
      'company',
      'companyLocation',
      'node',
    ],
    scenarioStatus: 'captured',
    assertionKinds: [
      'payload-shape',
      'downstream-read-parity',
      'aggregate-derivation',
      'connection-windowing',
      'local-staging',
    ],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes/b2b.rs'],
    proxyRequest: {
      documentPath: companyCreateRequestPath,
      variablesCapturePath: '$.operations.companyCreate.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live Shopify 2025-01 capture for B2B Company and CompanyLocation aggregate/relation reads after a disposable company, completed B2B draft order, and company-location catalogCreate. The replay earns every setup object through public GraphQL requests, then verifies totalSpent, ordersCount aliases, nested orders/draftOrders connections, lifetimeDuration, location currency, catalogs, and generic node reads from staged state.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'company-create-setup',
          capturePath: '$.operations.companyCreate.response.payload.data.companyCreate',
          proxyPath: '$.data.companyCreate',
          selectedPaths: ['$.company.name', '$.company.locations.nodes[0].name', '$.userErrors'],
        },
        {
          name: 'draft-order-create-b2b-setup',
          capturePath: '$.operations.draftOrderCreate.response.payload.data.draftOrderCreate',
          proxyPath: '$.data.draftOrderCreate',
          selectedPaths: ['$.userErrors'],
          proxyRequest: {
            documentPath: draftOrderCreateRequestPath,
            apiVersion,
            variables: {
              input: {
                purchasingEntity: {
                  purchasingCompany: {
                    companyId: { fromPrimaryProxyPath: '$.data.companyCreate.company.id' },
                    companyContactId: { fromPrimaryProxyPath: '$.data.companyCreate.company.mainContact.id' },
                    companyLocationId: {
                      fromPrimaryProxyPath: '$.data.companyCreate.company.locations.nodes[0].id',
                    },
                  },
                },
                email: { fromCapturePath: '$.operations.draftOrderCreate.variables.input.email' },
                note: { fromCapturePath: '$.operations.draftOrderCreate.variables.input.note' },
                tags: { fromCapturePath: '$.operations.draftOrderCreate.variables.input.tags' },
                visibleToCustomer: {
                  fromCapturePath: '$.operations.draftOrderCreate.variables.input.visibleToCustomer',
                },
                lineItems: { fromCapturePath: '$.operations.draftOrderCreate.variables.input.lineItems' },
              },
            },
          },
        },
        {
          name: 'draft-order-complete-b2b-setup',
          capturePath: '$.operations.draftOrderComplete.response.payload.data.draftOrderComplete',
          proxyPath: '$.data.draftOrderComplete',
          selectedPaths: ['$.userErrors'],
          proxyRequest: {
            documentPath: draftOrderCompleteRequestPath,
            apiVersion,
            variables: {
              id: {
                fromProxyResponse: 'draft-order-create-b2b-setup',
                path: '$.data.draftOrderCreate.draftOrder.id',
              },
              paymentPending: { fromCapturePath: '$.operations.draftOrderComplete.variables.paymentPending' },
            },
          },
        },
        {
          name: 'catalog-create-company-location-setup',
          capturePath: '$.operations.catalogCreate.response.payload.data.catalogCreate',
          proxyPath: '$.data.catalogCreate',
          selectedPaths: ['$.userErrors'],
          proxyRequest: {
            documentPath: catalogCreateRequestPath,
            apiVersion,
            variables: {
              input: {
                title: { fromCapturePath: '$.operations.catalogCreate.variables.input.title' },
                status: { fromCapturePath: '$.operations.catalogCreate.variables.input.status' },
                context: {
                  companyLocationIds: [{ fromPrimaryProxyPath: '$.data.companyCreate.company.locations.nodes[0].id' }],
                },
              },
            },
          },
        },
        {
          name: 'read-after-b2b-order-and-catalog-aggregates',
          capturePath: '$.operations.aggregateRead.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: aggregateReadRequestPath,
            apiVersion,
            variables: {
              companyId: { fromPrimaryProxyPath: '$.data.companyCreate.company.id' },
              locationId: { fromPrimaryProxyPath: '$.data.companyCreate.company.locations.nodes[0].id' },
            },
          },
          expectedDifferences: [
            {
              path: '$.company.id',
              matcher: 'shopify-gid:Company',
              reason: 'Live Shopify and local staging allocate different company IDs.',
            },
            {
              path: '$.companyLocation.id',
              matcher: 'shopify-gid:CompanyLocation',
              reason: 'Live Shopify and local staging allocate different company location IDs.',
            },
            {
              path: '$.company.lifetimeDuration',
              matcher: 'non-empty-string',
              reason:
                'Shopify computes lifetimeDuration from wall-clock indexing time; local replay computes it from the deterministic proxy clock.',
            },
            {
              path: '$.companyNode.lifetimeDuration',
              matcher: 'non-empty-string',
              reason:
                'Shopify computes lifetimeDuration from wall-clock indexing time; local replay computes it from the deterministic proxy clock.',
            },
            {
              path: '$.company.orders.nodes[*].id',
              matcher: 'shopify-gid:Order',
              reason: 'Live Shopify and local staging allocate different order IDs.',
            },
            {
              path: '$.company.orders.nodes[*].name',
              matcher: 'non-empty-string',
              reason: 'Live Shopify and local staging allocate order names from different store counters.',
            },
            {
              path: '$.companyLocation.orders.nodes[*].id',
              matcher: 'shopify-gid:Order',
              reason: 'Live Shopify and local staging allocate different order IDs.',
            },
            {
              path: '$.companyLocation.orders.nodes[*].name',
              matcher: 'non-empty-string',
              reason: 'Live Shopify and local staging allocate order names from different store counters.',
            },
            {
              path: '$.company.draftOrders.nodes[*].id',
              matcher: 'shopify-gid:DraftOrder',
              reason: 'Live Shopify and local staging allocate different draft order IDs.',
            },
            {
              path: '$.company.draftOrders.nodes[*].name',
              matcher: 'non-empty-string',
              reason: 'Live Shopify and local staging allocate draft order names from different store counters.',
            },
            {
              path: '$.companyLocation.draftOrders.nodes[*].id',
              matcher: 'shopify-gid:DraftOrder',
              reason: 'Live Shopify and local staging allocate different draft order IDs.',
            },
            {
              path: '$.companyLocation.draftOrders.nodes[*].name',
              matcher: 'non-empty-string',
              reason: 'Live Shopify and local staging allocate draft order names from different store counters.',
            },
            {
              path: '$.companyLocation.catalogs.nodes[*].id',
              matcher: 'shopify-gid:CompanyLocationCatalog',
              reason: 'Live Shopify and local staging allocate different catalog IDs.',
            },
            {
              path: '$.locationNode.catalogs.nodes[*].id',
              matcher: 'shopify-gid:CompanyLocationCatalog',
              reason: 'Live Shopify and local staging allocate different catalog IDs.',
            },
          ],
        },
      ],
    },
  };
}

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

let companyId: string | null = null;
let companyContactId: string | null = null;
let companyLocationId: string | null = null;
let draftOrderId: string | null = null;
let orderId: string | null = null;
let catalogId: string | null = null;
const cleanup: CaptureStep[] = [];

try {
  const companyCreate = await captureRequired(
    companyCreateDocument,
    companyVariables(stamp),
    'companyCreate',
    'companyCreate setup',
  );
  companyId = readStringAtPath(
    companyCreate.response.payload,
    ['data', 'companyCreate', 'company', 'id'],
    'company id',
  );
  companyContactId = readStringAtPath(
    companyCreate.response.payload,
    ['data', 'companyCreate', 'company', 'mainContact', 'id'],
    'main contact id',
  );
  companyLocationId = readStringAtPath(
    companyCreate.response.payload,
    ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
    'company location id',
  );

  const draftOrderCreate = await captureRequired(
    draftOrderCreateDocument,
    draftOrderVariables(stamp, companyId, companyContactId, companyLocationId),
    'draftOrderCreate',
    'draftOrderCreate setup',
  );
  draftOrderId = readStringAtPath(
    draftOrderCreate.response.payload,
    ['data', 'draftOrderCreate', 'draftOrder', 'id'],
    'draft order id',
  );

  const draftOrderComplete = await captureRequired(
    draftOrderCompleteDocument,
    draftOrderCompleteVariables(draftOrderId),
    'draftOrderComplete',
    'draftOrderComplete setup',
  );
  orderId = readStringAtPath(
    draftOrderComplete.response.payload,
    ['data', 'draftOrderComplete', 'draftOrder', 'order', 'id'],
    'completed order id',
  );

  const catalogCreateVariables = catalogVariables(stamp, companyLocationId);
  const catalogCreate = await captureRequired(
    catalogCreateDocument,
    catalogCreateVariables,
    'catalogCreate',
    'catalogCreate setup',
  );
  catalogId = readStringAtPath(
    catalogCreate.response.payload,
    ['data', 'catalogCreate', 'catalog', 'id'],
    'catalog id',
  );
  const catalogTitle = readStringAtPath(catalogCreateVariables, ['input', 'title'], 'catalog title');

  const aggregateRead = await captureAggregateReadWithRetry(
    aggregateReadVariables(companyId, companyLocationId),
    catalogTitle,
  );

  if (orderId) {
    cleanup.push(
      await cleanupCapture(
        orderCancelDocument,
        { orderId, reason: 'OTHER', notifyCustomer: false, restock: true },
        `orderCancel cleanup ${orderId}`,
      ),
    );
    cleanup.push(await cleanupCapture(orderDeleteDocument, { orderId }, `orderDelete cleanup ${orderId}`));
  } else if (draftOrderId) {
    cleanup.push(
      await cleanupCapture(
        draftOrderDeleteDocument,
        { input: { id: draftOrderId } },
        `draftOrderDelete cleanup ${draftOrderId}`,
      ),
    );
  }
  if (catalogId) {
    cleanup.push(await cleanupCapture(catalogDeleteDocument, { id: catalogId }, `catalogDelete cleanup ${catalogId}`));
  }
  if (companyId) {
    cleanup.push(await cleanupCapture(companyDeleteDocument, { id: companyId }, `companyDelete cleanup ${companyId}`));
  }

  await writeText(companyCreateRequestPath, trimGraphql(companyCreateDocument));
  await writeText(draftOrderCreateRequestPath, trimGraphql(draftOrderCreateDocument));
  await writeText(draftOrderCompleteRequestPath, trimGraphql(draftOrderCompleteDocument));
  await writeText(catalogCreateRequestPath, trimGraphql(catalogCreateDocument));
  await writeText(aggregateReadRequestPath, trimGraphql(aggregateReadDocument));
  await writeJson(specPath, specPayload());
  await writeJson(fixturePath, {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes:
      'Captured from live Shopify Admin GraphQL. The scenario creates a disposable B2B company/location/contact, completes one B2B draft order as paid, creates one CompanyLocationCatalog, reads aggregate/relation fields and nested order/draft-order connections after indexing, then records best-effort cleanup attempts.',
    operations: {
      companyCreate,
      draftOrderCreate,
      draftOrderComplete,
      catalogCreate,
      aggregateRead,
    },
    cleanup,
  });

  console.log(`Wrote ${fixturePath}`);
  console.log(`Wrote ${specPath}`);
  console.log(`Wrote ${companyCreateRequestPath}`);
  console.log(`Wrote ${draftOrderCreateRequestPath}`);
  console.log(`Wrote ${draftOrderCompleteRequestPath}`);
  console.log(`Wrote ${catalogCreateRequestPath}`);
  console.log(`Wrote ${aggregateReadRequestPath}`);
} catch (error) {
  if (orderId) {
    try {
      cleanup.push(
        await cleanupCapture(
          orderCancelDocument,
          { orderId, reason: 'OTHER', notifyCustomer: false, restock: true },
          `orderCancel cleanup after failure ${orderId}`,
        ),
      );
      cleanup.push(
        await cleanupCapture(orderDeleteDocument, { orderId }, `orderDelete cleanup after failure ${orderId}`),
      );
    } catch (cleanupError) {
      console.error(`Failed to clean up order ${orderId}:`, cleanupError);
    }
  } else if (draftOrderId) {
    try {
      cleanup.push(
        await cleanupCapture(
          draftOrderDeleteDocument,
          { input: { id: draftOrderId } },
          `draftOrderDelete cleanup after failure ${draftOrderId}`,
        ),
      );
    } catch (cleanupError) {
      console.error(`Failed to clean up draft order ${draftOrderId}:`, cleanupError);
    }
  }
  if (catalogId) {
    try {
      cleanup.push(
        await cleanupCapture(catalogDeleteDocument, { id: catalogId }, `catalog cleanup after failure ${catalogId}`),
      );
    } catch (cleanupError) {
      console.error(`Failed to clean up catalog ${catalogId}:`, cleanupError);
    }
  }
  if (companyId) {
    try {
      cleanup.push(
        await cleanupCapture(companyDeleteDocument, { id: companyId }, `company cleanup after failure ${companyId}`),
      );
    } catch (cleanupError) {
      console.error(`Failed to clean up company ${companyId}:`, cleanupError);
    }
  }
  if (cleanup.length > 0) {
    await writeJson(path.join(fixtureDir, `${scenarioId}-cleanup.json`), {
      scenarioId,
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cleanup,
    });
  }
  throw error;
}
