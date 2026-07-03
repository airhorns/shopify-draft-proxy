/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlResult = {
  status: number;
  payload: JsonRecord;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphqlRequest: (query: string, variables?: Record<string, unknown>) => Promise<GraphqlResult>;
};

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'order-catalog-count-read.json');

// The proxy answers `orders`/`ordersCount` reads against a pre-existing catalog by
// forwarding these exact documents upstream and returning the observed response (the local
// catalog engine returns None when no orders are staged, so dispatch falls through to the
// live-hybrid forward). Forward the byte-identical documents here so the recorded cassettes
// match the proxy's emitted queries verbatim — this replaces the previous
// `seedOrderCatalogFromCapture` pre-injection of the catalog and the hand-synthesized
// next-page cassette query that never byte-matched.
const mainQuery = await readFile(
  path.join('config', 'parity-requests', 'orders', 'order-catalog-count-read.graphql'),
  'utf8',
);
const nextPageQuery = await readFile(
  path.join('config', 'parity-requests', 'orders', 'order-catalog-count-next-page.graphql'),
  'utf8',
);

const CATALOG_TAG = 'merchant-realistic';
const TAG_QUERY = `tag:${CATALOG_TAG}`;

function readPath(value: unknown, segments: (string | number)[]): unknown {
  let current: unknown = value;
  for (const segment of segments) {
    if (current === null || current === undefined) return undefined;
    current = (current as Record<string | number, unknown>)[segment];
  }
  return current;
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`expected non-empty string for ${label}, got ${JSON.stringify(value)}`);
  }
  return value;
}

function assertNoGraphqlErrors(label: string, result: GraphqlResult): void {
  const errors = result.payload['errors'];
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`${label} returned GraphQL errors: ${JSON.stringify(errors)}`);
  }
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} returned HTTP ${result.status}: ${JSON.stringify(result.payload)}`);
  }
}

// A paid, unfulfilled, merchant-realistic-tagged order used to top up the live catalog when
// the store has fewer than two so the pagination/count assertions are non-trivial.
const orderCreateMutation = `#graphql
  mutation OrderCatalogCountReadOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        createdAt
        displayFinancialStatus
        displayFulfillmentStatus
        tags
      }
      userErrors {
        field
        message
      }
    }
  }
`;

function makeCatalogOrderVariables(stamp: number, index: number): Record<string, unknown> {
  return {
    order: {
      email: `catalog-count-${stamp}-${index}@example.com`,
      note: 'order-catalog-count-read parity catalog order',
      tags: ['parity-probe', 'order-catalog-count', CATALOG_TAG],
      test: true,
      lineItems: [
        {
          title: `Catalog count item ${index}`,
          quantity: 1,
          priceSet: { shopMoney: { amount: '10.00', currencyCode: 'CAD' } },
          requiresShipping: false,
          taxable: false,
          sku: `catalog-count-${stamp}-${index}`,
        },
      ],
      transactions: [
        {
          kind: 'SALE',
          status: 'SUCCESS',
          gateway: 'manual',
          test: true,
          amountSet: { shopMoney: { amount: '10.00', currencyCode: 'CAD' } },
        },
      ],
    },
    options: null,
  };
}

const countProbeQuery = `#graphql
  query OrderCatalogCountReadProbe($tag: String!) {
    orders(first: 3, query: $tag, sortKey: CREATED_AT, reverse: true) {
      nodes { id name }
    }
    ordersCount(query: $tag, limit: null) { count precision }
  }
`;

async function ensureCatalog(): Promise<void> {
  const probe = await runGraphqlRequest(countProbeQuery, { tag: TAG_QUERY });
  assertNoGraphqlErrors('catalog probe', probe);
  const existing = Number(readPath(probe.payload, ['data', 'ordersCount', 'count']) ?? 0);
  if (existing >= 2) {
    console.log(`[catalog] ${existing} ${CATALOG_TAG} orders already present; not creating any`);
    return;
  }
  const stamp = Date.now();
  for (let i = existing; i < 2; i += 1) {
    const created = await runGraphqlRequest(orderCreateMutation, makeCatalogOrderVariables(stamp, i));
    assertNoGraphqlErrors(`catalog order create ${i}`, created);
    const userErrors = readPath(created.payload, ['data', 'orderCreate', 'userErrors']);
    if (Array.isArray(userErrors) && userErrors.length > 0) {
      throw new Error(`catalog order create ${i} userErrors: ${JSON.stringify(userErrors)}`);
    }
    const name = requireString(
      readPath(created.payload, ['data', 'orderCreate', 'order', 'name']),
      'created order.name',
    );
    console.log(`[catalog] created ${CATALOG_TAG} order ${name}`);
  }
}

await ensureCatalog();

// Resolve a name query that matches a real order in the catalog (the numeric part of the
// newest order's name), so the byName branch is a meaningful non-empty filter.
const newestProbe = await runGraphqlRequest(countProbeQuery, { tag: TAG_QUERY });
assertNoGraphqlErrors('newest probe', newestProbe);
const newestName = requireString(
  readPath(newestProbe.payload, ['data', 'orders', 'nodes', 0, 'name']),
  'newest order.name',
);
const newestId = requireString(readPath(newestProbe.payload, ['data', 'orders', 'nodes', 0, 'id']), 'newest order.id');
const nameNumber = newestName.replace(/[^0-9]/g, '');
const nameQuery = `name:${nameNumber}`;
const idNumber = newestId.split('/').at(-1);
if (idNumber === undefined || idNumber.length === 0) {
  throw new Error(`expected numeric tail in newest order id ${newestId}`);
}
const idQuery = `id:${idNumber}`;
const idMissQuery = 'id:999999999999999999';
const combinedQuery = `${idQuery} ${TAG_QUERY} financial_status:paid fulfillment_status:unfulfilled`;

const mainVariables = {
  tagQuery: TAG_QUERY,
  nameQuery,
  statusQuery: 'financial_status:paid fulfillment_status:unfulfilled',
  idQuery,
  idMissQuery,
  combinedQuery,
  pageSize: 1,
  seedSize: 2,
  countLimit: 1,
  unlimited: null,
};

const mainResult = await runGraphqlRequest(mainQuery, mainVariables);
assertNoGraphqlErrors('order catalog main query', mainResult);

const recentEndCursor = requireString(
  readPath(mainResult.payload, ['data', 'recent', 'pageInfo', 'endCursor']),
  'recent.pageInfo.endCursor',
);

const nextPageVariables = {
  tagQuery: TAG_QUERY,
  pageSize: 1,
  after: recentEndCursor,
};

const nextPageResult = await runGraphqlRequest(nextPageQuery, nextPageVariables);
assertNoGraphqlErrors('order catalog next-page query', nextPageResult);

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  variables: mainVariables,
  response: mainResult.payload,
  nextPage: {
    variables: nextPageVariables,
    response: nextPageResult.payload,
  },
  upstreamCalls: [
    {
      operationName: 'OrderCatalogCountRead',
      variables: mainVariables,
      query: mainQuery,
      response: { status: mainResult.status, body: mainResult.payload },
    },
    {
      operationName: 'OrderCatalogNextPage',
      variables: nextPageVariables,
      query: nextPageQuery,
      response: { status: nextPageResult.status, body: nextPageResult.payload },
    },
  ],
});

console.log(
  JSON.stringify(
    {
      ok: true,
      fixturePath,
      storeDomain,
      apiVersion,
      nameQuery,
      idQuery,
      idMissQuery,
      combinedQuery,
      taggedCount: readPath(mainResult.payload, ['data', 'exactTaggedCount']),
      idMissCount: readPath(mainResult.payload, ['data', 'idMissCount']),
      combinedAndCount: readPath(mainResult.payload, ['data', 'combinedAndCount']),
      recentNodeCount: Array.isArray(readPath(mainResult.payload, ['data', 'recent', 'nodes']))
        ? (readPath(mainResult.payload, ['data', 'recent', 'nodes']) as unknown[]).length
        : null,
      recentHasNextPage: readPath(mainResult.payload, ['data', 'recent', 'pageInfo', 'hasNextPage']),
      nextPageNodeCount: Array.isArray(readPath(nextPageResult.payload, ['data', 'nextPage', 'nodes']))
        ? (readPath(nextPageResult.payload, ['data', 'nextPage', 'nodes']) as unknown[]).length
        : null,
    },
    null,
    2,
  ),
);
