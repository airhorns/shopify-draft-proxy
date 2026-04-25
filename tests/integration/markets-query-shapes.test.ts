import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { hydrateMarketsFromUpstreamResponse } from '../../src/proxy/markets.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

const repoRoot = process.cwd();
const fixtureRoot = 'fixtures/conformance/very-big-test-store.myshopify.com/2026-04';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

type CapturedMarketEdge = {
  cursor: string;
  node: {
    id: string;
    name: string;
  };
};

type MarketsCatalogFixture = {
  data: {
    markets: {
      edges: CapturedMarketEdge[];
    };
  };
};

function readText(relativePath: string): string {
  return readFileSync(resolve(repoRoot, relativePath), 'utf8');
}

function readJson<T>(relativePath: string): T {
  return JSON.parse(readText(relativePath)) as T;
}

describe('Markets query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it.each([
    {
      name: 'markets catalog',
      documentPath: 'config/parity-requests/markets-catalog-read.graphql',
      variablesPath: 'config/parity-requests/markets-catalog-read.variables.json',
      fixturePath: `${fixtureRoot}/markets-catalog.json`,
    },
    {
      name: 'market detail',
      documentPath: 'config/parity-requests/market-detail-read.graphql',
      variablesPath: 'config/parity-requests/market-detail-read.variables.json',
      fixturePath: `${fixtureRoot}/market-detail.json`,
    },
    {
      name: 'market catalogs',
      documentPath: 'config/parity-requests/market-catalogs-read.graphql',
      variablesPath: 'config/parity-requests/market-catalogs-read.variables.json',
      fixturePath: `${fixtureRoot}/market-catalogs.json`,
    },
    {
      name: 'market web presences',
      documentPath: 'config/parity-requests/market-web-presences-read.graphql',
      variablesPath: 'config/parity-requests/market-web-presences-read.variables.json',
      fixturePath: `${fixtureRoot}/market-web-presences.json`,
    },
    {
      name: 'markets resolved values',
      documentPath: 'config/parity-requests/markets-resolved-values-read.graphql',
      variablesPath: 'config/parity-requests/markets-resolved-values-read.variables.json',
      fixturePath: `${fixtureRoot}/markets-resolved-values.json`,
    },
  ])('serves captured $name from local snapshot state', async ({ documentPath, variablesPath, fixturePath }) => {
    const document = readText(documentPath);
    const variables = readJson<Record<string, unknown>>(variablesPath);
    const fixture = readJson<{ data: Record<string, unknown> }>(fixturePath);
    hydrateMarketsFromUpstreamResponse(document, variables, fixture);

    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('snapshot Markets reads must not fetch upstream'));
    const app = createApp(config);

    const response = await request(app.callback()).post('/admin/api/2026-04/graphql.json').send({
      query: document,
      variables,
    });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({ data: fixture.data });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('returns Shopify-like null and empty connections for absent Markets snapshot data', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('snapshot Markets reads must not fetch upstream'));
    const app = createApp(config);

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query EmptyMarkets($id: ID!) {
            market(id: $id) {
              id
            }
            markets(first: 3) {
              nodes {
                id
              }
              edges {
                cursor
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            webPresences(first: 3) {
              edges {
                cursor
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
          }
        `,
        variables: {
          id: 'gid://shopify/Market/0',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        market: null,
        markets: {
          nodes: [],
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        webPresences: {
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
      },
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('applies Markets connection pagination, reverse ordering, and root filters from normalized state', async () => {
    const document = readText('config/parity-requests/markets-catalog-read.graphql');
    const variables = readJson<Record<string, unknown>>('config/parity-requests/markets-catalog-read.variables.json');
    const fixture = readJson<MarketsCatalogFixture>(`${fixtureRoot}/markets-catalog.json`);
    hydrateMarketsFromUpstreamResponse(document, variables, fixture);

    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('snapshot Markets reads must not fetch upstream'));
    const app = createApp(config);
    const [canadaEdge, usEdge] = fixture.data.markets.edges;

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query MarketsConnectionWindows(
            $first: Int
            $after: String
            $last: Int
            $before: String
            $reverse: Boolean
            $type: MarketType
          ) {
            firstPage: markets(first: $first, type: $type) {
              nodes {
                id
                name
              }
              edges {
                cursor
                node {
                  id
                  name
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            afterPage: markets(first: $first, after: $after) {
              edges {
                cursor
                node {
                  name
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            beforePage: markets(last: $last, before: $before) {
              edges {
                cursor
                node {
                  name
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            reversed: markets(first: $first, reverse: $reverse) {
              edges {
                cursor
                node {
                  name
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            inactive: markets(first: 5, query: "status:DRAFT") {
              nodes {
                id
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
          }
        `,
        variables: {
          first: 1,
          after: canadaEdge?.cursor,
          last: 1,
          before: usEdge?.cursor,
          reverse: true,
          type: 'REGION',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      firstPage: {
        nodes: [{ id: canadaEdge?.node.id, name: 'Canada' }],
        edges: [{ cursor: canadaEdge?.cursor, node: { id: canadaEdge?.node.id, name: 'Canada' } }],
        pageInfo: {
          hasNextPage: true,
          hasPreviousPage: false,
          startCursor: canadaEdge?.cursor,
          endCursor: canadaEdge?.cursor,
        },
      },
      afterPage: {
        edges: [{ cursor: usEdge?.cursor, node: { name: 'Conformance US' } }],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: true,
          startCursor: usEdge?.cursor,
          endCursor: usEdge?.cursor,
        },
      },
      beforePage: {
        edges: [{ cursor: canadaEdge?.cursor, node: { name: 'Canada' } }],
        pageInfo: {
          hasNextPage: true,
          hasPreviousPage: false,
          startCursor: canadaEdge?.cursor,
          endCursor: canadaEdge?.cursor,
        },
      },
      reversed: {
        edges: [{ cursor: usEdge?.cursor, node: { name: 'Conformance US' } }],
        pageInfo: {
          hasNextPage: true,
          hasPreviousPage: false,
          startCursor: usEdge?.cursor,
          endCursor: usEdge?.cursor,
        },
      },
      inactive: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('applies captured Markets query filters from normalized state', async () => {
    const document = readText('config/parity-requests/markets-catalog-read.graphql');
    const variables = readJson<Record<string, unknown>>('config/parity-requests/markets-catalog-read.variables.json');
    const fixture = readJson<MarketsCatalogFixture>(`${fixtureRoot}/markets-catalog.json`);
    hydrateMarketsFromUpstreamResponse(document, variables, fixture);

    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('snapshot Markets reads must not fetch upstream'));
    const app = createApp(config);
    const [canadaEdge, usEdge] = fixture.data.markets.edges;

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query MarketsQueryFilters($idQuery: String!) {
            byName: markets(first: 5, query: "name:'Canada'") {
              nodes {
                id
                name
              }
            }
            byId: markets(first: 5, query: $idQuery) {
              nodes {
                id
                name
              }
            }
            byMarketTypeAndCondition: markets(
              first: 5
              query: "market_type:REGION market_condition_types:REGION"
              sortKey: NAME
            ) {
              nodes {
                id
                name
              }
            }
            defaultText: markets(first: 5, query: "Conformance") {
              nodes {
                id
                name
              }
            }
          }
        `,
        variables: {
          idQuery: `id:${usEdge?.node.id}`,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      byName: {
        nodes: [{ id: canadaEdge?.node.id, name: 'Canada' }],
      },
      byId: {
        nodes: [{ id: usEdge?.node.id, name: 'Conformance US' }],
      },
      byMarketTypeAndCondition: {
        nodes: [
          { id: canadaEdge?.node.id, name: 'Canada' },
          { id: usEdge?.node.id, name: 'Conformance US' },
        ],
      },
      defaultText: {
        nodes: [{ id: usEdge?.node.id, name: 'Conformance US' }],
      },
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });
});
