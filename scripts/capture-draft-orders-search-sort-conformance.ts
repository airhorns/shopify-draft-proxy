/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';
import { captureDraftProxyShopPricingHydrate } from './support/shopify/runtime-hydration-capture.js';

type CaptureEntry = {
  document: string;
  variables: JsonRecord;
  response: JsonRecord;
};

const scenarioId = 'draft-orders-search-sort-staged';
const cap = await createConformanceCapture();
const shopPricingHydrate = await captureDraftProxyShopPricingHydrate((query, variables) =>
  cap.runGraphqlRequest(query, variables),
);
const fixturePath = cap.fixturePath('orders', 'draft-orders-search-sort-staged.json');
const specPath = 'config/parity-specs/orders/draftOrders-search-sort-staged.json';
const createDocumentPath = 'config/parity-requests/orders/draftOrders-search-sort-create.graphql';
const updateDocumentPath = 'config/parity-requests/orders/draftOrders-search-sort-update.graphql';
const readDocumentPath = 'config/parity-requests/orders/draftOrders-search-sort-read.graphql';

const createDocument = await cap.readRequestRaw('orders', 'draftOrders-search-sort-create.graphql');
const updateDocument = await cap.readRequestRaw('orders', 'draftOrders-search-sort-update.graphql');
const readDocument = await cap.readRequestRaw('orders', 'draftOrders-search-sort-read.graphql');

const deleteDocument = `#graphql
  mutation DraftOrdersSearchSortCleanup($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const customerDeleteDocument = `#graphql
  mutation DraftOrdersSearchSortCustomerCleanup($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

function sleep(milliseconds: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}

async function capture(document: string, variables: JsonRecord, label: string): Promise<CaptureEntry> {
  const response = await cap.run(document, variables, label);
  return { document, variables, response };
}

function dataRoot(payload: JsonRecord, key: string): JsonRecord {
  const root = readRecord(readRecord(payload['data'])?.[key]);
  if (!root) throw new Error(`Missing data.${key}: ${JSON.stringify(payload, null, 2)}`);
  return root;
}

function draftOrderIdFromCreate(entry: CaptureEntry): string {
  return requireString(
    readRecord(dataRoot(entry.response, 'draftOrderCreate')['draftOrder'])?.['id'],
    'draftOrderCreate.draftOrder.id',
  );
}

function draftOrderCustomerIdFromCreate(entry: CaptureEntry): string | null {
  const draftOrder = readRecord(dataRoot(entry.response, 'draftOrderCreate')['draftOrder']);
  const customerId = readRecord(draftOrder?.['customer'])?.['id'];
  return typeof customerId === 'string' && customerId.length > 0 ? customerId : null;
}

function draftOrderEmails(payload: JsonRecord): string[] {
  const draftOrders = readRecord(readRecord(payload['data'])?.['draftOrders']);
  return readArray(draftOrders?.['nodes'])
    .map((node) => readRecord(node)?.['email'])
    .filter((email): email is string => typeof email === 'string');
}

function draftOrdersEndCursor(payload: JsonRecord): string {
  const draftOrders = readRecord(readRecord(payload['data'])?.['draftOrders']);
  return requireString(readRecord(draftOrders?.['pageInfo'])?.['endCursor'], 'draftOrders.pageInfo.endCursor');
}

async function captureReadUntil(
  variables: JsonRecord,
  expectedEmails: string[],
  label: string,
): Promise<CaptureEntry & { attempts: number }> {
  let latest: CaptureEntry | null = null;
  for (let attempt = 1; attempt <= 24; attempt += 1) {
    latest = await capture(readDocument, variables, label);
    const emails = draftOrderEmails(latest.response);
    if (JSON.stringify(emails) === JSON.stringify(expectedEmails)) return { ...latest, attempts: attempt };
    await sleep(500);
  }
  throw new Error(
    `${label} did not return expected emails ${JSON.stringify(expectedEmails)}; latest=${JSON.stringify(latest, null, 2)}`,
  );
}

function readTarget(
  name: string,
  captureKey: string,
  selectedPaths: string[],
  variablesPath = `$.reads.${captureKey}.variables`,
): JsonRecord {
  return {
    name,
    capturePath: `$.reads.${captureKey}.response.data`,
    proxyPath: '$.data',
    preserveProxyState: true,
    selectedPaths,
    proxyRequest: {
      documentPath: readDocumentPath,
      variablesCapturePath: variablesPath,
      apiVersion: cap.apiVersion,
    },
  };
}

const stampTag = `draftsort${cap.stamp}`;
const alphaTag = `alpha${cap.stamp}`;
const betaTag = `beta${cap.stamp}`;
const alphaEmail = `draft-sort-alpha-${cap.stamp}@example.com`;
const betaEmail = `draft-sort-beta-${cap.stamp}@example.com`;
const createdDraftOrderIds: string[] = [];
const createdCustomerIds: string[] = [];
let cleanupResults: unknown = { draftOrders: [], customers: [] };
let cleanupDone = false;

async function cleanupCreatedRecords(): Promise<JsonRecord> {
  cleanupDone = true;
  const draftOrders = await Promise.allSettled(
    [...new Set(createdDraftOrderIds)].map((id) => cap.runGraphqlRequest(deleteDocument, { input: { id } })),
  );
  const customers = await Promise.allSettled(
    [...new Set(createdCustomerIds)].map((id) => cap.runGraphqlRequest(customerDeleteDocument, { input: { id } })),
  );
  return { draftOrders, customers };
}

const alphaCreateVariables = {
  input: {
    email: alphaEmail,
    tags: [stampTag, alphaTag],
    note: `Alpha searchable ${cap.stamp}`,
    lineItems: [
      {
        title: `Alpha Search Needle ${cap.stamp}`,
        quantity: 1,
        originalUnitPrice: '12.50',
      },
    ],
  },
} satisfies JsonRecord;

const betaCreateVariables = {
  input: {
    email: betaEmail,
    tags: [stampTag, betaTag],
    note: `Beta searchable ${cap.stamp}`,
    lineItems: [
      {
        title: `Beta Search Haystack ${cap.stamp}`,
        quantity: 1,
        originalUnitPrice: '5.00',
      },
    ],
  },
} satisfies JsonRecord;

try {
  const alphaCreate = await capture(createDocument, alphaCreateVariables, 'draft order search/sort alpha create');
  cap.mutationRoot(alphaCreate.response, 'draftOrderCreate', 'alpha draftOrderCreate');
  const alphaId = draftOrderIdFromCreate(alphaCreate);
  createdDraftOrderIds.push(alphaId);
  const alphaCustomerId = draftOrderCustomerIdFromCreate(alphaCreate);
  if (alphaCustomerId) createdCustomerIds.push(alphaCustomerId);

  const betaCreate = await capture(createDocument, betaCreateVariables, 'draft order search/sort beta create');
  cap.mutationRoot(betaCreate.response, 'draftOrderCreate', 'beta draftOrderCreate');
  const betaId = draftOrderIdFromCreate(betaCreate);
  createdDraftOrderIds.push(betaId);
  const betaCustomerId = draftOrderCustomerIdFromCreate(betaCreate);
  if (betaCustomerId) createdCustomerIds.push(betaCustomerId);

  await sleep(1500);
  const alphaUpdateVariables = {
    id: alphaId,
    input: { note: `Alpha touched for updatedAt sort ${cap.stamp}` },
  } satisfies JsonRecord;
  const alphaUpdate = await capture(updateDocument, alphaUpdateVariables, 'draft order search/sort alpha update');
  cap.mutationRoot(alphaUpdate.response, 'draftOrderUpdate', 'alpha draftOrderUpdate');

  const allById = [alphaEmail, betaEmail];
  const reversedById = [betaEmail, alphaEmail];
  const defaultId = await captureReadUntil(
    { first: 2, after: null, query: `tag:${stampTag}`, sortKey: null, reverse: false },
    allById,
    'draftOrders default ID sort',
  );
  const idReverse = await captureReadUntil(
    { first: 2, after: null, query: `tag:${stampTag}`, sortKey: 'ID', reverse: true },
    reversedById,
    'draftOrders ID reverse sort',
  );
  const updatedAtAsc = await captureReadUntil(
    { first: 2, after: null, query: `tag:${stampTag}`, sortKey: 'UPDATED_AT', reverse: false },
    reversedById,
    'draftOrders UPDATED_AT ascending sort',
  );
  const firstPage = await captureReadUntil(
    { first: 1, after: null, query: `tag:${stampTag}`, sortKey: 'ID', reverse: false },
    [alphaEmail],
    'draftOrders first page',
  );
  const secondPage = await captureReadUntil(
    {
      first: 1,
      after: draftOrdersEndCursor(firstPage.response),
      query: `tag:${stampTag}`,
      sortKey: 'ID',
      reverse: false,
    },
    [betaEmail],
    'draftOrders second page',
  );
  const statusTag = await captureReadUntil(
    { first: 2, after: null, query: `status:open tag:${alphaTag}`, sortKey: 'ID', reverse: false },
    [alphaEmail],
    'draftOrders status and tag query',
  );
  const createdAt = await captureReadUntil(
    { first: 2, after: null, query: `created_at:>=2024-01-01 tag:${stampTag}`, sortKey: 'ID', reverse: false },
    allById,
    'draftOrders created_at comparator query',
  );
  const updatedAt = await captureReadUntil(
    { first: 2, after: null, query: `updated_at:>=2024-01-01 tag:${stampTag}`, sortKey: 'ID', reverse: false },
    allById,
    'draftOrders updated_at comparator query',
  );
  const freeText = await captureReadUntil(
    { first: 2, after: null, query: `Needle ${cap.stamp}`, sortKey: 'ID', reverse: false },
    [alphaEmail],
    'draftOrders default text query',
  );
  const unknownAndKnown = await captureReadUntil(
    { first: 2, after: null, query: `notadraftfield:ignored tag:${stampTag}`, sortKey: 'ID', reverse: false },
    allById,
    'draftOrders unknown field ignored with known tag query',
  );
  const reads = {
    defaultId,
    idReverse,
    updatedAtAsc,
    firstPage,
    secondPage,
    statusTag,
    createdAt,
    updatedAt,
    freeText,
    unknownAndKnown,
  };

  cleanupResults = await cleanupCreatedRecords();

  await cap.writeJson(fixturePath, {
    scenarioId,
    apiVersion: cap.apiVersion,
    storeDomain: cap.storeDomain,
    recordedAt: new Date().toISOString(),
    source: 'live-shopify-admin-graphql',
    notes:
      'Live staged draftOrders search/sort evidence using two disposable draft orders. Shopify 2026-04 on this shop emitted invalid-field warnings for total_price during manual probing, so total_price comparator behavior is covered by local runtime tests and documented separately instead of asserted as strict live parity here.',
    setup: {
      alphaCreate,
      betaCreate,
      alphaUpdate,
    },
    reads,
    cleanup: cleanupResults,
    upstreamCalls: [shopPricingHydrate],
  });

  const twoNodeListPaths = [
    '$.draftOrders.nodes[0].email',
    '$.draftOrders.nodes[1].email',
    '$.draftOrders.pageInfo.hasNextPage',
    '$.draftOrders.pageInfo.hasPreviousPage',
    '$.draftOrdersCount.count',
    '$.draftOrdersCount.precision',
  ];
  const oneNodeListPaths = [
    '$.draftOrders.nodes[0].email',
    '$.draftOrders.pageInfo.hasNextPage',
    '$.draftOrders.pageInfo.hasPreviousPage',
    '$.draftOrdersCount.count',
    '$.draftOrdersCount.precision',
  ];

  await cap.writeJson(specPath, {
    scenarioId,
    operationNames: ['draftOrderCreate', 'draftOrderUpdate', 'draftOrders', 'draftOrdersCount'],
    scenarioStatus: 'captured',
    assertionKinds: [
      'runtime-staging',
      'query-filter-parity',
      'sort-order-parity',
      'pagination-parity',
      'count-parity',
    ],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes.rs'],
    proxyRequest: {
      documentPath: createDocumentPath,
      variablesCapturePath: '$.setup.alphaCreate.variables',
      apiVersion: cap.apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'alpha-create-user-errors',
          capturePath: '$.setup.alphaCreate.response.data.draftOrderCreate.userErrors',
          proxyPath: '$.data.draftOrderCreate.userErrors',
        },
        {
          name: 'beta-create-user-errors',
          capturePath: '$.setup.betaCreate.response.data.draftOrderCreate.userErrors',
          proxyPath: '$.data.draftOrderCreate.userErrors',
          preserveProxyState: true,
          proxyRequest: {
            documentPath: createDocumentPath,
            variablesCapturePath: '$.setup.betaCreate.variables',
            apiVersion: cap.apiVersion,
          },
        },
        {
          name: 'alpha-update-user-errors',
          capturePath: '$.setup.alphaUpdate.response.data.draftOrderUpdate.userErrors',
          proxyPath: '$.data.draftOrderUpdate.userErrors',
          preserveProxyState: true,
          proxyRequest: {
            documentPath: updateDocumentPath,
            variables: {
              id: { fromPrimaryProxyPath: '$.data.draftOrderCreate.draftOrder.id' },
              input: { note: `Alpha touched for updatedAt sort ${cap.stamp}` },
            },
            apiVersion: cap.apiVersion,
          },
        },
        readTarget('default-id-sort-and-count', 'defaultId', twoNodeListPaths),
        readTarget('id-reverse-sort', 'idReverse', twoNodeListPaths),
        readTarget('updated-at-ascending-sort', 'updatedAtAsc', twoNodeListPaths),
        readTarget('first-page-window', 'firstPage', oneNodeListPaths),
        {
          ...readTarget('second-page-window', 'secondPage', oneNodeListPaths),
          proxyRequest: {
            documentPath: readDocumentPath,
            variables: {
              first: 1,
              after: { fromProxyResponse: 'first-page-window', path: '$.data.draftOrders.pageInfo.endCursor' },
              query: `tag:${stampTag}`,
              sortKey: 'ID',
              reverse: false,
            },
            apiVersion: cap.apiVersion,
          },
        },
        readTarget('status-and-tag-filter', 'statusTag', oneNodeListPaths),
        readTarget('created-at-comparator-filter', 'createdAt', twoNodeListPaths),
        readTarget('updated-at-comparator-filter', 'updatedAt', twoNodeListPaths),
        readTarget('default-text-filter', 'freeText', oneNodeListPaths),
        readTarget('unknown-field-ignored-with-known-filter', 'unknownAndKnown', twoNodeListPaths),
      ],
    },
    notes:
      'Uses public GraphQL draftOrderCreate/draftOrderUpdate setup, then compares staged draftOrders sortKey/reverse, pagination pageInfo/counts, accepted search filters, and Shopify observed unknown-field-ignore behavior.',
  });
} finally {
  if (!cleanupDone && (createdDraftOrderIds.length > 0 || createdCustomerIds.length > 0))
    cleanupResults = await cleanupCreatedRecords();
}

console.log(
  JSON.stringify(
    {
      ok: true,
      fixturePath,
      specPath,
      storeDomain: cap.storeDomain,
      apiVersion: cap.apiVersion,
      cleanupResults,
    } satisfies JsonRecord,
    null,
    2,
  ),
);
