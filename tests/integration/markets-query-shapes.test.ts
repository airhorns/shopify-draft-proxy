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
const marketPriceListFixtureRoot = 'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04';

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

type MarketCatalogsFixture = {
  data: {
    catalogs: {
      edges: Array<{
        cursor: string;
        node: {
          id: string;
          title: string;
          status: string;
          priceList: {
            id: string;
            name: string;
            currency: string;
          } | null;
          publication: {
            id: string;
            autoPublish: boolean;
          } | null;
          markets: {
            edges: Array<{
              cursor: string;
              node: {
                id: string;
                name: string;
                handle: string;
                status: string;
                type: string;
              };
            }>;
          };
        };
      }>;
    };
  };
};

type PriceListFixture = {
  data: {
    priceList: {
      id: string;
      prices: {
        edges: Array<{
          cursor: string;
          node: {
            originType: string;
            variant: {
              id: string;
              sku: string | null;
              product: {
                id: string;
                title: string;
              };
            };
          };
        }>;
      };
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
      documentPath: 'config/parity-requests/markets/markets-catalog-read.graphql',
      variablesPath: 'config/parity-requests/markets/markets-catalog-read.variables.json',
      fixturePath: `${fixtureRoot}/markets-catalog.json`,
    },
    {
      name: 'market detail',
      documentPath: 'config/parity-requests/markets/market-detail-read.graphql',
      variablesPath: 'config/parity-requests/markets/market-detail-read.variables.json',
      fixturePath: `${fixtureRoot}/market-detail.json`,
    },
    {
      name: 'market catalogs',
      documentPath: 'config/parity-requests/markets/market-catalogs-read.graphql',
      variablesPath: 'config/parity-requests/markets/market-catalogs-read.variables.json',
      fixturePath: `${fixtureRoot}/market-catalogs.json`,
    },
    {
      name: 'market web presences',
      documentPath: 'config/parity-requests/markets/market-web-presences-read.graphql',
      variablesPath: 'config/parity-requests/markets/market-web-presences-read.variables.json',
      fixturePath: `${fixtureRoot}/market-web-presences.json`,
    },
    {
      name: 'markets resolved values',
      documentPath: 'config/parity-requests/markets/markets-resolved-values-read.graphql',
      variablesPath: 'config/parity-requests/markets/markets-resolved-values-read.variables.json',
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

  it('resolves marketsResolvedValues from hydrated buyer-country market state', async () => {
    hydrateMarketsFromUpstreamResponse(
      readText('config/parity-requests/markets/markets-catalog-read.graphql'),
      readJson<Record<string, unknown>>('config/parity-requests/markets/markets-catalog-read.variables.json'),
      readJson(`${fixtureRoot}/markets-catalog.json`),
    );

    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('snapshot Markets reads must not fetch upstream'));
    const app = createApp(config);

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query LocalResolvedValues {
            us: marketsResolvedValues(buyerSignal: { countryCode: US }) {
              currencyCode
              priceInclusivity {
                dutiesIncluded
                taxesIncluded
              }
              catalogs(first: 5) {
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
              webPresences(first: 5) {
                nodes {
                  id
                  subfolderSuffix
                }
                pageInfo {
                  hasNextPage
                  hasPreviousPage
                  startCursor
                  endCursor
                }
              }
            }
            de: marketsResolvedValues(buyerSignal: { countryCode: DE }) {
              currencyCode
              priceInclusivity {
                dutiesIncluded
                taxesIncluded
              }
              catalogs(first: 5) {
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
              webPresences(first: 5) {
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
          }
        `,
      });

    expect(response.status).toBe(200);
    expect(response.body.data.us).toEqual({
      currencyCode: 'CAD',
      priceInclusivity: {
        dutiesIncluded: false,
        taxesIncluded: false,
      },
      catalogs: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
      webPresences: {
        nodes: [
          {
            id: 'gid://shopify/MarketWebPresence/33131921641',
            subfolderSuffix: null,
          },
        ],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: 'eyJsYXN0X2lkIjozMzEzMTkyMTY0MSwibGFzdF92YWx1ZSI6MzMxMzE5MjE2NDF9',
          endCursor: 'eyJsYXN0X2lkIjozMzEzMTkyMTY0MSwibGFzdF92YWx1ZSI6MzMxMzE5MjE2NDF9',
        },
      },
    });
    expect(response.body.data.de).toEqual({
      currencyCode: 'EUR',
      priceInclusivity: {
        dutiesIncluded: false,
        taxesIncluded: false,
      },
      catalogs: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
      webPresences: {
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

  it('returns Shopify-like marketsResolvedValues invalid buyer-signal variable errors locally', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('snapshot Markets validation must not fetch upstream'));
    const app = createApp(config);

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query InvalidResolvedValues($buyerSignal: BuyerSignalInput!) {
            marketsResolvedValues(buyerSignal: $buyerSignal) {
              currencyCode
            }
          }
        `,
        variables: {
          buyerSignal: {
            countryCode: 'AQ',
          },
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toBeUndefined();
    expect(response.body.errors).toEqual([
      expect.objectContaining({
        message: expect.stringContaining('Variable $buyerSignal of type BuyerSignalInput! was provided invalid value'),
        extensions: expect.objectContaining({
          code: 'INVALID_VARIABLE',
          value: {
            countryCode: 'AQ',
          },
          problems: [
            expect.objectContaining({
              path: ['countryCode'],
              explanation: expect.stringContaining('Expected "AQ" to be one of:'),
            }),
          ],
        }),
      }),
    ]);
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it.each([
    {
      name: 'market catalog detail and count',
      documentPath: 'config/parity-requests/markets/market-catalog-detail-read.graphql',
      variablesPath: 'config/parity-requests/markets/market-catalog-detail-read.variables.json',
      fixturePath: `${marketPriceListFixtureRoot}/market-catalog-detail.json`,
    },
    {
      name: 'price list detail',
      documentPath: 'config/parity-requests/markets/price-list-detail-read.graphql',
      variablesPath: 'config/parity-requests/markets/price-list-detail-read.variables.json',
      fixturePath: `${marketPriceListFixtureRoot}/price-list-detail.json`,
    },
    {
      name: 'filtered price list prices',
      documentPath: 'config/parity-requests/markets/price-list-prices-filtered-read.graphql',
      variablesPath: 'config/parity-requests/markets/price-list-prices-filtered-read.variables.json',
      fixturePath: `${marketPriceListFixtureRoot}/price-list-prices-filtered.json`,
    },
    {
      name: 'price lists catalog',
      documentPath: 'config/parity-requests/markets/price-lists-read.graphql',
      variablesPath: 'config/parity-requests/markets/price-lists-read.variables.json',
      fixturePath: `${marketPriceListFixtureRoot}/price-lists.json`,
    },
  ])('serves captured $name roots from local snapshot state', async ({ documentPath, variablesPath, fixturePath }) => {
    const document = readText(documentPath);
    const variables = readJson<Record<string, unknown>>(variablesPath);
    const fixture = readJson<{ data: Record<string, unknown> }>(fixturePath);
    hydrateMarketsFromUpstreamResponse(document, variables, fixture);

    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('snapshot catalog and price-list reads must not fetch'));
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
            catalog(id: $catalogId) {
              id
            }
            catalogs(first: 3) {
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
            catalogsCount(type: MARKET, limit: 10) {
              count
              precision
            }
            priceList(id: $priceListId) {
              id
            }
            priceLists(first: 3) {
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
          }
        `,
        variables: {
          id: 'gid://shopify/Market/0',
          catalogId: 'gid://shopify/MarketCatalog/0',
          priceListId: 'gid://shopify/PriceList/0',
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
        catalog: null,
        catalogs: {
          nodes: [],
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        catalogsCount: {
          count: 0,
          precision: 'EXACT',
        },
        priceList: null,
        priceLists: {
          nodes: [],
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
    const document = readText('config/parity-requests/markets/markets-catalog-read.graphql');
    const variables = readJson<Record<string, unknown>>(
      'config/parity-requests/markets/markets-catalog-read.variables.json',
    );
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
    const document = readText('config/parity-requests/markets/markets-catalog-read.graphql');
    const variables = readJson<Record<string, unknown>>(
      'config/parity-requests/markets/markets-catalog-read.variables.json',
    );
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

  it('applies catalog pagination, search filters, counts, and MarketCatalog nested market fields from normalized state', async () => {
    const document = readText('config/parity-requests/markets/market-catalogs-read.graphql');
    const variables = readJson<Record<string, unknown>>(
      'config/parity-requests/markets/market-catalogs-read.variables.json',
    );
    const fixture = readJson<MarketCatalogsFixture>(`${marketPriceListFixtureRoot}/market-catalogs.json`);
    hydrateMarketsFromUpstreamResponse(document, variables, fixture);

    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('snapshot Catalog reads must not fetch upstream'));
    const app = createApp(config);
    const [catalogEdge] = fixture.data.catalogs.edges;
    const catalog = catalogEdge!.node;
    const [marketEdge] = catalog.markets.edges;

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query CatalogWindows($catalogId: ID!, $after: String, $marketQuery: String!) {
            catalog(id: $catalogId) {
              __typename
              id
              title
              status
              operations {
                __typename
              }
              marketsCount {
                count
                precision
              }
              markets(first: 1) {
                edges {
                  cursor
                  node {
                    id
                    name
                    handle
                    status
                    type
                  }
                }
              }
              priceList {
                id
                name
                currency
              }
              publication {
                id
                autoPublish
              }
            }
            firstPage: catalogs(first: 1, type: MARKET, sortKey: TITLE) {
              edges {
                cursor
                node {
                  id
                  title
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            afterPage: catalogs(first: 1, after: $after, type: MARKET) {
              edges {
                node {
                  id
                }
              }
            }
            activeCount: catalogsCount(type: MARKET, query: "status:ACTIVE", limit: 1) {
              count
              precision
            }
            byMarket: catalogs(first: 5, query: $marketQuery) {
              nodes {
                id
                title
              }
            }
          }
        `,
        variables: {
          catalogId: catalog.id,
          after: catalogEdge!.cursor,
          marketQuery: `market_id:${marketEdge!.node.id}`,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      catalog: {
        __typename: 'MarketCatalog',
        id: catalog.id,
        title: catalog.title,
        status: catalog.status,
        operations: [],
        marketsCount: {
          count: 1,
          precision: 'EXACT',
        },
        markets: {
          edges: [
            {
              cursor: marketEdge!.cursor,
              node: marketEdge!.node,
            },
          ],
        },
        priceList: catalog.priceList,
        publication: catalog.publication,
      },
      firstPage: {
        edges: [{ cursor: catalogEdge!.cursor, node: { id: catalog.id, title: catalog.title } }],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: catalogEdge!.cursor,
          endCursor: catalogEdge!.cursor,
        },
      },
      afterPage: {
        edges: [],
      },
      activeCount: {
        count: 1,
        precision: 'EXACT',
      },
      byMarket: {
        nodes: [{ id: catalog.id, title: catalog.title }],
      },
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('applies price-list pagination and price filters while preserving product variant price relationships', async () => {
    const detailDocument = readText('config/parity-requests/markets/price-list-detail-read.graphql');
    const detailVariables = readJson<Record<string, unknown>>(
      'config/parity-requests/markets/price-list-detail-read.variables.json',
    );
    const detailFixture = readJson<PriceListFixture>(`${marketPriceListFixtureRoot}/price-list-detail.json`);
    hydrateMarketsFromUpstreamResponse(detailDocument, detailVariables, detailFixture);

    const listDocument = readText('config/parity-requests/markets/price-lists-read.graphql');
    const listVariables = readJson<Record<string, unknown>>(
      'config/parity-requests/markets/price-lists-read.variables.json',
    );
    const listFixture = readJson<{ data: Record<string, unknown> }>(`${marketPriceListFixtureRoot}/price-lists.json`);
    hydrateMarketsFromUpstreamResponse(listDocument, listVariables, listFixture);

    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('snapshot PriceList reads must not fetch upstream'));
    const app = createApp(config);
    const [priceEdge] = detailFixture.data.priceList.prices.edges;
    const variantNumericId = priceEdge!.node.variant.id.split('/').at(-1);

    const response = await request(app.callback())
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query PriceListWindows($priceListId: ID!, $priceQuery: String!) {
            priceList(id: $priceListId) {
              id
              name
              currency
              fixedPricesCount
              parent {
                adjustment {
                  type
                  value
                }
              }
              catalog {
                id
                title
                status
              }
              prices(first: 5, query: $priceQuery) {
                edges {
                  cursor
                  node {
                    originType
                    variant {
                      id
                      sku
                      product {
                        id
                        title
                      }
                    }
                  }
                }
                pageInfo {
                  hasNextPage
                  hasPreviousPage
                  startCursor
                  endCursor
                }
              }
            }
            priceLists(first: 1, reverse: true) {
              edges {
                cursor
                node {
                  id
                  name
                  currency
                }
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
          priceListId: detailFixture.data.priceList.id,
          priceQuery: `variant_id:${variantNumericId}`,
        },
      });

    const allPriceListEdges = (
      listFixture.data['priceLists'] as {
        edges: Array<{ cursor: string; node: Record<string, unknown> }>;
      }
    ).edges;
    const reversedFirst = allPriceListEdges.at(-1)!;

    expect(response.status).toBe(200);
    expect(response.body.data.priceList.prices.edges).toEqual([
      {
        cursor: priceEdge!.cursor,
        node: {
          originType: priceEdge!.node.originType,
          variant: priceEdge!.node.variant,
        },
      },
    ]);
    expect(response.body.data.priceList.prices.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: priceEdge!.cursor,
      endCursor: priceEdge!.cursor,
    });
    expect(response.body.data.priceLists).toEqual({
      edges: [
        {
          cursor: reversedFirst.cursor,
          node: {
            id: reversedFirst.node['id'],
            name: reversedFirst.node['name'],
            currency: reversedFirst.node['currency'],
          },
        },
      ],
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: false,
        startCursor: reversedFirst.cursor,
        endCursor: reversedFirst.cursor,
      },
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });
});
