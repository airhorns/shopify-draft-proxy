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
const fixtureRoot = 'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/markets';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const MARKET_FIELDS = `#graphql
  fragment LifecycleMarketFields on Market {
    id
    name
    handle
    status
    enabled
    type
    conditions {
      conditionTypes
      regionsCondition {
        applicationLevel
        regions(first: 5) {
          edges {
            cursor
            node {
              __typename
              id
              name
              ... on MarketRegionCountry {
                code
                currency {
                  currencyCode
                  currencyName
                  enabled
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
      companyLocationsCondition {
        applicationLevel
        companyLocations(first: 5) {
          edges {
            cursor
            node {
              id
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
      locationsCondition {
        applicationLevel
        locations(first: 5) {
          edges {
            cursor
            node {
              id
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
    }
    currencySettings {
      baseCurrency {
        currencyCode
        currencyName
        enabled
      }
      localCurrencies
      roundingEnabled
    }
    priceInclusions {
      inclusiveDutiesPricingStrategy
      inclusiveTaxPricingStrategy
    }
  }
`;

const PRICE_LIST_FIELDS = `#graphql
  fragment LifecyclePriceListFields on PriceList {
    __typename
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
    }
    quantityRules(first: 10) {
      edges {
        cursor
        node {
          minimum
          maximum
          increment
          isDefault
          originType
          productVariant {
            id
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
    prices(first: 10, originType: FIXED) {
      edges {
        cursor
        node {
          price {
            amount
            currencyCode
          }
          compareAtPrice {
            amount
            currencyCode
          }
          originType
          variant {
            id
            sku
            product {
              id
              title
            }
          }
          quantityPriceBreaks(first: 10) {
            edges {
              cursor
              node {
                id
                minimumQuantity
                price {
                  amount
                  currencyCode
                }
                variant {
                  id
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
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

function seedCatalogMarkets(): { canadaMarketId: string; usMarketId: string } {
  const canadaMarketId = 'gid://shopify/Market/101';
  const usMarketId = 'gid://shopify/Market/202';
  store.upsertBaseMarkets([
    {
      id: canadaMarketId,
      __typename: 'Market',
      name: 'Canada',
      handle: 'canada',
      status: 'ACTIVE',
      enabled: true,
      type: 'REGION',
    },
    {
      id: usMarketId,
      __typename: 'Market',
      name: 'United States',
      handle: 'united-states',
      status: 'ACTIVE',
      enabled: true,
      type: 'REGION',
    },
  ]);

  return { canadaMarketId, usMarketId };
}

const WEB_PRESENCE_FIELDS = `#graphql
  fragment LifecycleWebPresenceFields on MarketWebPresence {
    id
    subfolderSuffix
    domain {
      id
      host
      url
      sslEnabled
    }
    rootUrls {
      locale
      url
    }
    defaultLocale {
      locale
      name
      primary
      published
    }
    alternateLocales {
      locale
      name
      primary
      published
    }
    markets(first: 5) {
      nodes {
        id
        name
        handle
        status
        type
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

function readJson<T>(relativePath: string): T {
  return JSON.parse(readFileSync(resolve(repoRoot, relativePath), 'utf8')) as T;
}

describe('Markets lifecycle staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages marketCreate, marketUpdate, and marketDelete locally with read-after-write and meta visibility', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('market lifecycle must not proxy'));
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `${MARKET_FIELDS}
          mutation CreateMarket($input: MarketCreateInput!) {
            marketCreate(input: $input) {
              market {
                ...LifecycleMarketFields
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          input: {
            name: 'Codex Canada',
            handle: 'codex-canada',
            status: 'DRAFT',
            conditions: {
              regionsCondition: {
                regions: [{ countryCode: 'CA' }],
              },
            },
            currencySettings: {
              baseCurrency: 'CAD',
              localCurrencies: false,
              roundingEnabled: true,
            },
            priceInclusions: {
              dutiesPricingStrategy: 'ADD_DUTIES_AT_CHECKOUT',
              taxPricingStrategy: 'INCLUDES_TAXES_IN_PRICE',
            },
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.marketCreate.userErrors).toEqual([]);
    expect(createResponse.body.data.marketCreate.market).toMatchObject({
      id: expect.stringMatching(/^gid:\/\/shopify\/Market\//),
      name: 'Codex Canada',
      handle: 'codex-canada',
      status: 'DRAFT',
      enabled: false,
      type: 'REGION',
      conditions: {
        conditionTypes: ['REGION'],
        regionsCondition: {
          applicationLevel: 'SPECIFIED',
          regions: {
            edges: [
              {
                cursor: expect.stringMatching(/^gid:\/\/shopify\/MarketRegionCountry\//),
                node: {
                  __typename: 'MarketRegionCountry',
                  id: expect.stringMatching(/^gid:\/\/shopify\/MarketRegionCountry\//),
                  name: 'Canada',
                  code: 'CA',
                  currency: {
                    currencyCode: 'CAD',
                    currencyName: 'Canadian Dollar',
                    enabled: true,
                  },
                },
              },
            ],
          },
        },
      },
      currencySettings: {
        baseCurrency: {
          currencyCode: 'CAD',
          currencyName: 'Canadian Dollar',
          enabled: true,
        },
        localCurrencies: false,
        roundingEnabled: true,
      },
      priceInclusions: {
        inclusiveDutiesPricingStrategy: 'ADD_DUTIES_AT_CHECKOUT',
        inclusiveTaxPricingStrategy: 'INCLUDES_TAXES_IN_PRICE',
      },
    });

    const marketId = createResponse.body.data.marketCreate.market.id as string;
    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${MARKET_FIELDS}
          mutation UpdateMarket($id: ID!, $input: MarketUpdateInput!) {
            marketUpdate(id: $id, input: $input) {
              market {
                ...LifecycleMarketFields
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          id: marketId,
          input: {
            name: 'Codex Canada Renamed',
            handle: 'codex-canada-renamed',
            status: 'DRAFT',
            currencySettings: {
              baseCurrency: 'CAD',
              localCurrencies: true,
            },
            removePriceInclusions: true,
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.marketUpdate).toMatchObject({
      market: {
        id: marketId,
        name: 'Codex Canada Renamed',
        handle: 'codex-canada-renamed',
        status: 'DRAFT',
        enabled: false,
        currencySettings: {
          baseCurrency: {
            currencyCode: 'CAD',
          },
          localCurrencies: true,
          roundingEnabled: true,
        },
        priceInclusions: null,
      },
      userErrors: [],
    });

    const readAfterUpdateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${MARKET_FIELDS}
          query ReadMarket($id: ID!) {
            market(id: $id) {
              ...LifecycleMarketFields
            }
            markets(first: 5, query: "name:'Renamed'") {
              nodes {
                id
                name
                handle
                status
              }
            }
          }
        `,
        variables: { id: marketId },
      });

    expect(readAfterUpdateResponse.status).toBe(200);
    expect(readAfterUpdateResponse.body.data.market).toMatchObject({
      id: marketId,
      name: 'Codex Canada Renamed',
      handle: 'codex-canada-renamed',
      status: 'DRAFT',
    });
    expect(readAfterUpdateResponse.body.data.markets.nodes).toEqual([
      {
        id: marketId,
        name: 'Codex Canada Renamed',
        handle: 'codex-canada-renamed',
        status: 'DRAFT',
      },
    ]);

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeleteMarket($id: ID!) {
            marketDelete(id: $id) {
              deletedId
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: { id: marketId },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.marketDelete).toEqual({
      deletedId: marketId,
      userErrors: [],
    });

    const readAfterDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query ReadDeletedMarket($id: ID!) {
            market(id: $id) {
              id
            }
            markets(first: 5) {
              nodes {
                id
              }
            }
          }
        `,
        variables: { id: marketId },
      });

    expect(readAfterDeleteResponse.status).toBe(200);
    expect(readAfterDeleteResponse.body.data.market).toBeNull();
    expect(readAfterDeleteResponse.body.data.markets.nodes).not.toContainEqual({ id: marketId });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries).toHaveLength(3);
    expect(logResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
    ]);
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'marketCreate',
      'marketUpdate',
      'marketDelete',
    ]);
    expect(logResponse.body.entries[0].requestBody.variables.input.name).toBe('Codex Canada');

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.markets).toEqual({});
    expect(stateResponse.body.stagedState.deletedMarketIds).toEqual({ [marketId]: true });

    const commitResponse = await request(app).post('/__meta/commit');
    expect(commitResponse.status).toBe(200);
    expect(commitResponse.body.attempts[0]).toMatchObject({
      operationName: 'marketCreate',
      success: false,
      upstreamStatus: null,
    });

    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });

  it('returns MarketUserError shapes for invalid market lifecycle inputs without staging records', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('market lifecycle validation must not proxy'));
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation CreateMarket($input: MarketCreateInput!) {
            marketCreate(input: $input) {
              market {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          input: {
            name: 'Duplicate Handle',
            handle: 'duplicate-market',
            conditions: {
              regionsCondition: {
                regions: [{ countryCode: 'US' }],
              },
            },
          },
        },
      });

    expect(createResponse.status).toBe(200);
    const duplicateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DuplicateHandle($input: MarketCreateInput!) {
            marketCreate(input: $input) {
              market {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          input: {
            name: 'Duplicate Handle Two',
            handle: 'duplicate-market',
          },
        },
      });

    expect(duplicateResponse.status).toBe(200);
    expect(duplicateResponse.body.data.marketCreate).toEqual({
      market: null,
      userErrors: [
        {
          field: ['input', 'handle'],
          message: "Handle 'duplicate-market' has already been taken",
          code: 'TAKEN',
        },
      ],
    });

    const invalidCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation InvalidCreate($input: MarketCreateInput!) {
            marketCreate(input: $input) {
              market {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          input: {
            name: '',
            enabled: true,
            status: 'DRAFT',
            conditions: {
              regionsCondition: {
                applicationLevel: 'SPECIFIED',
                regions: [],
              },
              companyLocationsCondition: {
                applicationLevel: 'ALL',
              },
            },
            currencySettings: {
              baseCurrency: 'ZZZ',
            },
          },
        },
      });

    expect(invalidCreateResponse.status).toBe(200);
    expect(invalidCreateResponse.body.data.marketCreate).toEqual({
      market: null,
      userErrors: [
        { field: ['input', 'name'], message: "Name can't be blank", code: 'BLANK' },
        {
          field: ['input', 'name'],
          message: 'Name is too short (minimum is 2 characters)',
          code: 'TOO_SHORT',
        },
        {
          field: ['input', 'enabled'],
          message: 'Invalid combination of status and enabled',
          code: 'INVALID_STATUS_AND_ENABLED_COMBINATION',
        },
        {
          field: ['input', 'conditions', 'regionsCondition', 'regions'],
          message: 'Specified conditions cannot be empty',
          code: 'SPECIFIED_CONDITIONS_CANNOT_BE_EMPTY',
        },
        {
          field: ['input', 'conditions'],
          message: 'The specified conditions are not compatible with each other',
          code: 'INCOMPATIBLE_CONDITIONS',
        },
        {
          field: ['input', 'currencySettings', 'baseCurrency'],
          message: 'The specified currency is not supported',
          code: 'UNSUPPORTED_CURRENCY',
        },
      ],
    });

    const unknownUpdateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UnknownUpdate($id: ID!, $input: MarketUpdateInput!) {
            marketUpdate(id: $id, input: $input) {
              market {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          id: 'gid://shopify/Market/999999',
          input: {
            name: 'Nope',
          },
        },
      });

    expect(unknownUpdateResponse.status).toBe(200);
    expect(unknownUpdateResponse.body.data.marketUpdate).toEqual({
      market: null,
      userErrors: [
        {
          field: ['id'],
          message: 'Market does not exist',
          code: 'MARKET_NOT_FOUND',
        },
      ],
    });

    const unknownDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UnknownDelete($id: ID!) {
            marketDelete(id: $id) {
              deletedId
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          id: 'gid://shopify/Market/999999',
        },
      });

    expect(unknownDeleteResponse.status).toBe(200);
    expect(unknownDeleteResponse.body.data.marketDelete).toEqual({
      deletedId: null,
      userErrors: [
        {
          field: ['id'],
          message: 'Market does not exist',
          code: 'MARKET_NOT_FOUND',
        },
      ],
    });

    const unsafeDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeleteOnlyActiveRegion($id: ID!) {
            marketDelete(id: $id) {
              deletedId
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          id: createResponse.body.data.marketCreate.market.id,
        },
      });

    expect(unsafeDeleteResponse.status).toBe(200);
    expect(unsafeDeleteResponse.body.data.marketDelete).toEqual({
      deletedId: null,
      userErrors: [
        {
          field: ['id'],
          message: "Can't delete, disable, or change the type of the last region market",
          code: 'MUST_HAVE_AT_LEAST_ONE_ACTIVE_REGION_MARKET',
        },
      ],
    });

    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('stages market catalog lifecycle and context mutations locally with downstream reads and meta visibility', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('catalog lifecycle must not proxy'));
    const { canadaMarketId, usMarketId } = seedCatalogMarkets();
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `#graphql
          mutation CatalogCreate($input: CatalogCreateInput!) {
            catalogCreate(input: $input) {
              catalog {
                __typename
                id
                ... on MarketCatalog {
                  title
                  status
                  marketsCount {
                    count
                    precision
                  }
                  markets(first: 5) {
                    nodes {
                      id
                      name
                    }
                  }
                  publication {
                    id
                  }
                  priceList {
                    id
                  }
                  operations {
                    __typename
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
        `,
        variables: {
          input: {
            title: 'Codex Market Catalog',
            status: 'ACTIVE',
            context: {
              marketIds: [canadaMarketId],
            },
            publicationId: 'gid://shopify/Publication/303',
            priceListId: 'gid://shopify/PriceList/404',
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.catalogCreate.userErrors).toEqual([]);
    expect(createResponse.body.data.catalogCreate.catalog).toMatchObject({
      __typename: 'MarketCatalog',
      id: expect.stringMatching(/^gid:\/\/shopify\/MarketCatalog\//),
      title: 'Codex Market Catalog',
      status: 'ACTIVE',
      marketsCount: {
        count: 1,
        precision: 'EXACT',
      },
      markets: {
        nodes: [{ id: canadaMarketId, name: 'Canada' }],
      },
      publication: {
        id: 'gid://shopify/Publication/303',
      },
      priceList: {
        id: 'gid://shopify/PriceList/404',
      },
      operations: [],
    });

    const catalogId = createResponse.body.data.catalogCreate.catalog.id as string;

    const readAfterCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query CatalogRead($catalogId: ID!, $marketId: ID!) {
            catalog(id: $catalogId) {
              __typename
              id
              ... on MarketCatalog {
                title
                status
                markets(first: 5) {
                  nodes {
                    id
                    name
                  }
                }
              }
            }
            catalogs(first: 10, type: MARKET, query: "title:'Codex Market Catalog'") {
              nodes {
                id
                title
              }
            }
            catalogsCount(type: MARKET, query: "market_id:gid://shopify/Market/101") {
              count
              precision
            }
            market(id: $marketId) {
              id
              catalogs(first: 5) {
                nodes {
                  id
                  title
                }
              }
            }
          }
        `,
        variables: { catalogId, marketId: canadaMarketId },
      });

    expect(readAfterCreateResponse.status).toBe(200);
    expect(readAfterCreateResponse.body.data).toMatchObject({
      catalog: {
        __typename: 'MarketCatalog',
        id: catalogId,
        title: 'Codex Market Catalog',
        status: 'ACTIVE',
        markets: {
          nodes: [{ id: canadaMarketId, name: 'Canada' }],
        },
      },
      catalogs: {
        nodes: [{ id: catalogId, title: 'Codex Market Catalog' }],
      },
      catalogsCount: {
        count: 1,
        precision: 'EXACT',
      },
      market: {
        id: canadaMarketId,
        catalogs: {
          nodes: [{ id: catalogId, title: 'Codex Market Catalog' }],
        },
      },
    });

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation CatalogUpdate($id: ID!, $input: CatalogUpdateInput!) {
            catalogUpdate(id: $id, input: $input) {
              catalog {
                id
                ... on MarketCatalog {
                  title
                  status
                  markets(first: 5) {
                    nodes {
                      id
                    }
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
        `,
        variables: {
          id: catalogId,
          input: {
            title: 'Codex Market Catalog Renamed',
            status: 'DRAFT',
            context: {
              marketIds: [canadaMarketId, usMarketId],
            },
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.catalogUpdate).toEqual({
      catalog: {
        id: catalogId,
        title: 'Codex Market Catalog Renamed',
        status: 'DRAFT',
        markets: {
          nodes: [{ id: canadaMarketId }, { id: usMarketId }],
        },
      },
      userErrors: [],
    });

    const contextUpdateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation CatalogContextUpdate(
            $catalogId: ID!
            $contextsToAdd: CatalogContextInput
            $contextsToRemove: CatalogContextInput
          ) {
            catalogContextUpdate(
              catalogId: $catalogId
              contextsToAdd: $contextsToAdd
              contextsToRemove: $contextsToRemove
            ) {
              catalog {
                id
                ... on MarketCatalog {
                  markets(first: 5) {
                    nodes {
                      id
                      name
                    }
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
        `,
        variables: {
          catalogId,
          contextsToAdd: null,
          contextsToRemove: {
            marketIds: [canadaMarketId],
          },
        },
      });

    expect(contextUpdateResponse.status).toBe(200);
    expect(contextUpdateResponse.body.data.catalogContextUpdate).toEqual({
      catalog: {
        id: catalogId,
        markets: {
          nodes: [{ id: usMarketId, name: 'United States' }],
        },
      },
      userErrors: [],
    });

    const readAfterContextResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query ContextRead($catalogId: ID!, $canadaMarketId: ID!, $usMarketId: ID!) {
            catalog(id: $catalogId) {
              id
              ... on MarketCatalog {
                markets(first: 5) {
                  nodes {
                    id
                  }
                }
              }
            }
            canada: market(id: $canadaMarketId) {
              catalogs(first: 5) {
                nodes {
                  id
                }
              }
            }
            us: market(id: $usMarketId) {
              catalogs(first: 5) {
                nodes {
                  id
                  title
                }
              }
            }
          }
        `,
        variables: { catalogId, canadaMarketId, usMarketId },
      });

    expect(readAfterContextResponse.status).toBe(200);
    expect(readAfterContextResponse.body.data).toEqual({
      catalog: {
        id: catalogId,
        markets: {
          nodes: [{ id: usMarketId }],
        },
      },
      canada: {
        catalogs: {
          nodes: [],
        },
      },
      us: {
        catalogs: {
          nodes: [{ id: catalogId, title: 'Codex Market Catalog Renamed' }],
        },
      },
    });

    const duplicateTitleResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DuplicateCatalog($input: CatalogCreateInput!) {
            catalogCreate(input: $input) {
              catalog {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          input: {
            title: 'Codex Market Catalog Renamed',
            context: {
              marketIds: [usMarketId],
            },
          },
        },
      });

    expect(duplicateTitleResponse.status).toBe(200);
    expect(duplicateTitleResponse.body.data.catalogCreate).toEqual({
      catalog: null,
      userErrors: [
        {
          field: ['input', 'title'],
          message: "Title 'Codex Market Catalog Renamed' has already been taken",
          code: 'TAKEN',
        },
      ],
    });

    const unsupportedContextResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UnsupportedCatalogContext($input: CatalogCreateInput!) {
            catalogCreate(input: $input) {
              catalog {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          input: {
            title: 'B2B Catalog',
            context: {
              companyLocationIds: ['gid://shopify/CompanyLocation/1'],
            },
          },
        },
      });

    expect(unsupportedContextResponse.status).toBe(200);
    expect(unsupportedContextResponse.body.data.catalogCreate).toEqual({
      catalog: null,
      userErrors: [
        {
          field: ['input', 'context', 'companyLocationIds'],
          message: 'Only market catalog contexts are supported locally',
          code: 'UNSUPPORTED_CONTEXT',
        },
        {
          field: ['input', 'context', 'marketIds'],
          message: 'At least one market is required',
          code: 'BLANK',
        },
      ],
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation CatalogDelete($id: ID!) {
            catalogDelete(id: $id) {
              deletedId
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: { id: catalogId },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.catalogDelete).toEqual({
      deletedId: catalogId,
      userErrors: [],
    });

    const readAfterDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query DeletedCatalog($catalogId: ID!, $marketId: ID!) {
            catalog(id: $catalogId) {
              id
            }
            catalogs(first: 5, query: "title:'Codex Market Catalog Renamed'") {
              nodes {
                id
              }
            }
            market(id: $marketId) {
              catalogs(first: 5) {
                nodes {
                  id
                }
              }
            }
          }
        `,
        variables: { catalogId, marketId: usMarketId },
      });

    expect(readAfterDeleteResponse.status).toBe(200);
    expect(readAfterDeleteResponse.body.data).toEqual({
      catalog: null,
      catalogs: {
        nodes: [],
      },
      market: {
        catalogs: {
          nodes: [],
        },
      },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries).toHaveLength(6);
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'CatalogCreate',
      'CatalogUpdate',
      'CatalogContextUpdate',
      'catalogCreate',
      'catalogCreate',
      'CatalogDelete',
    ]);
    expect(logResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
    ]);
    expect(logResponse.body.entries[0].requestBody.variables.input.title).toBe('Codex Market Catalog');

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.catalogs).toEqual({});
    expect(stateResponse.body.stagedState.deletedCatalogIds).toEqual({ [catalogId]: true });

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages webPresenceCreate and webPresenceUpdate locally with read-after-write and meta visibility', async () => {
    hydrateMarketsFromUpstreamResponse(
      'query SeedMarketsResolvedValues { marketsResolvedValues { webPresences { edges { node { id } } } } }',
      {},
      readJson(`${fixtureRoot}/markets-resolved-values.json`),
    );

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('web presence staging must not proxy'));
    const app = createApp(config).callback();
    const marketId = 'gid://shopify/Market/35532308713';

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${WEB_PRESENCE_FIELDS}
          mutation CreateWebPresence($input: WebPresenceCreateInput!) {
            webPresenceCreate(input: $input) {
              webPresence {
                ...LifecycleWebPresenceFields
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          input: {
            defaultLocale: 'en',
            alternateLocales: ['fr'],
            subfolderSuffix: 'ca',
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.webPresenceCreate.userErrors).toEqual([]);
    expect(createResponse.body.data.webPresenceCreate.webPresence).toMatchObject({
      id: expect.stringMatching(/^gid:\/\/shopify\/MarketWebPresence\//),
      subfolderSuffix: 'ca',
      domain: null,
      rootUrls: [
        { locale: 'en', url: 'https://very-big-test-store.myshopify.com/en-ca' },
        { locale: 'fr', url: 'https://very-big-test-store.myshopify.com/fr-ca' },
      ],
      defaultLocale: {
        locale: 'en',
        name: 'English',
        primary: true,
        published: true,
      },
      alternateLocales: [
        {
          locale: 'fr',
          name: 'French',
          primary: false,
          published: true,
        },
      ],
      markets: {
        nodes: [],
      },
    });

    const webPresenceId = createResponse.body.data.webPresenceCreate.webPresence.id as string;
    const associateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation AssociateWebPresence($id: ID!, $input: MarketUpdateInput!) {
            marketUpdate(id: $id, input: $input) {
              market {
                id
                webPresences(first: 5) {
                  nodes {
                    id
                    subfolderSuffix
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
        `,
        variables: {
          id: marketId,
          input: {
            webPresencesToAdd: [webPresenceId],
          },
        },
      });

    expect(associateResponse.status).toBe(200);
    expect(associateResponse.body.data.marketUpdate).toMatchObject({
      market: {
        id: marketId,
        webPresences: {
          nodes: expect.arrayContaining([{ id: webPresenceId, subfolderSuffix: 'ca' }]),
        },
      },
      userErrors: [],
    });

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${WEB_PRESENCE_FIELDS}
          mutation UpdateWebPresence($id: ID!, $input: WebPresenceUpdateInput!) {
            webPresenceUpdate(id: $id, input: $input) {
              webPresence {
                ...LifecycleWebPresenceFields
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          id: webPresenceId,
          input: {
            defaultLocale: 'fr',
            alternateLocales: ['en'],
            subfolderSuffix: 'frca',
          },
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.webPresenceUpdate).toMatchObject({
      webPresence: {
        id: webPresenceId,
        subfolderSuffix: 'frca',
        rootUrls: [
          { locale: 'fr', url: 'https://very-big-test-store.myshopify.com/fr-frca' },
          { locale: 'en', url: 'https://very-big-test-store.myshopify.com/en-frca' },
        ],
        markets: {
          nodes: [
            {
              id: marketId,
              name: 'Conformance US',
              handle: 'conformance-us',
              status: 'ACTIVE',
              type: 'REGION',
            },
          ],
        },
      },
      userErrors: [],
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${WEB_PRESENCE_FIELDS}
          query ReadWebPresenceAfterWrite($marketId: ID!) {
            webPresences(first: 5) {
              nodes {
                ...LifecycleWebPresenceFields
              }
            }
            market(id: $marketId) {
              id
              webPresences(first: 5) {
                nodes {
                  id
                  subfolderSuffix
                  defaultLocale {
                    locale
                  }
                }
              }
            }
            marketsResolvedValues(buyerSignal: { countryCode: US }) {
              webPresences(first: 5) {
                nodes {
                  id
                  subfolderSuffix
                }
              }
            }
          }
        `,
        variables: { marketId },
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.webPresences.nodes).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: webPresenceId,
          subfolderSuffix: 'frca',
          defaultLocale: expect.objectContaining({ locale: 'fr' }),
          markets: {
            nodes: [
              expect.objectContaining({
                id: marketId,
              }),
            ],
            pageInfo: expect.any(Object),
          },
        }),
      ]),
    );
    expect(readResponse.body.data.market.webPresences.nodes).toEqual(
      expect.arrayContaining([{ id: webPresenceId, subfolderSuffix: 'frca', defaultLocale: { locale: 'fr' } }]),
    );
    expect(readResponse.body.data.marketsResolvedValues.webPresences.nodes).toEqual(
      expect.arrayContaining([{ id: webPresenceId, subfolderSuffix: 'frca' }]),
    );

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'webPresenceCreate',
      'marketUpdate',
      'webPresenceUpdate',
    ]);
    expect(logResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
    ]);

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.webPresences[webPresenceId].data).toMatchObject({
      id: webPresenceId,
      subfolderSuffix: 'frca',
    });
    expect(stateResponse.body.stagedState.markets[marketId].data.webPresences.edges).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          node: expect.objectContaining({
            id: webPresenceId,
            subfolderSuffix: 'frca',
          }),
        }),
      ]),
    );

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeleteWebPresence($id: ID!) {
            webPresenceDelete(id: $id) {
              deletedId
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: { id: webPresenceId },
      });

    expect(deleteResponse.status).toBe(200);
    expect(deleteResponse.body.data.webPresenceDelete).toEqual({
      deletedId: webPresenceId,
      userErrors: [],
    });

    const alreadyDeletedResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeleteWebPresenceAgain($id: ID!) {
            webPresenceDelete(id: $id) {
              deletedId
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: { id: webPresenceId },
      });

    expect(alreadyDeletedResponse.status).toBe(200);
    expect(alreadyDeletedResponse.body.data.webPresenceDelete).toEqual({
      deletedId: null,
      userErrors: [
        {
          field: ['id'],
          message: "The market web presence wasn't found.",
          code: 'WEB_PRESENCE_NOT_FOUND',
        },
      ],
    });

    const readAfterDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query ReadWebPresenceAfterDelete($marketId: ID!) {
            webPresences(first: 5) {
              nodes {
                id
                subfolderSuffix
              }
            }
            market(id: $marketId) {
              id
              webPresences(first: 5) {
                nodes {
                  id
                  subfolderSuffix
                }
              }
            }
            marketsResolvedValues(buyerSignal: { countryCode: US }) {
              webPresences(first: 5) {
                nodes {
                  id
                  subfolderSuffix
                }
              }
            }
          }
        `,
        variables: { marketId },
      });

    expect(readAfterDeleteResponse.status).toBe(200);
    expect(readAfterDeleteResponse.body.data.webPresences.nodes).not.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: webPresenceId })]),
    );
    expect(readAfterDeleteResponse.body.data.market.webPresences.nodes).not.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: webPresenceId })]),
    );
    expect(readAfterDeleteResponse.body.data.marketsResolvedValues.webPresences.nodes).not.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: webPresenceId })]),
    );

    const logAfterDeleteResponse = await request(app).get('/__meta/log');
    expect(logAfterDeleteResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'webPresenceCreate',
      'marketUpdate',
      'webPresenceUpdate',
      'webPresenceDelete',
      'webPresenceDelete',
    ]);
    expect(logAfterDeleteResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
    ]);

    const stateAfterDeleteResponse = await request(app).get('/__meta/state');
    expect(stateAfterDeleteResponse.body.stagedState.webPresences[webPresenceId]).toBeUndefined();
    expect(stateAfterDeleteResponse.body.stagedState.deletedWebPresenceIds).toEqual({ [webPresenceId]: true });
    expect(stateAfterDeleteResponse.body.stagedState.markets[marketId].data.webPresences.edges).not.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          node: expect.objectContaining({ id: webPresenceId }),
        }),
      ]),
    );

    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('resolves marketsResolvedValues from staged market catalog and web presence effects', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('resolved values staging must not proxy'));
    const app = createApp(config).callback();

    const marketResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${MARKET_FIELDS}
          mutation CreateResolvedMarket($input: MarketCreateInput!) {
            marketCreate(input: $input) {
              market {
                ...LifecycleMarketFields
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          input: {
            name: 'Codex Germany',
            handle: 'codex-germany',
            status: 'ACTIVE',
            conditions: {
              regionsCondition: {
                regions: [{ countryCode: 'DE' }],
              },
            },
            currencySettings: {
              baseCurrency: 'EUR',
              localCurrencies: false,
              roundingEnabled: true,
            },
            priceInclusions: {
              dutiesPricingStrategy: 'INCLUDES_DUTIES_IN_PRICE',
              taxPricingStrategy: 'INCLUDES_TAXES_IN_PRICE',
            },
          },
        },
      });

    expect(marketResponse.status).toBe(200);
    expect(marketResponse.body.data.marketCreate.userErrors).toEqual([]);
    const marketId = marketResponse.body.data.marketCreate.market.id as string;

    const catalogResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation CreateResolvedCatalog($input: CatalogCreateInput!) {
            catalogCreate(input: $input) {
              catalog {
                __typename
                id
                ... on MarketCatalog {
                  title
                  status
                  priceList {
                    id
                  }
                  markets(first: 5) {
                    nodes {
                      id
                    }
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
        `,
        variables: {
          input: {
            title: 'Codex Germany Catalog',
            status: 'ACTIVE',
            context: {
              marketIds: [marketId],
            },
            priceListId: 'gid://shopify/PriceList/919',
          },
        },
      });

    expect(catalogResponse.status).toBe(200);
    expect(catalogResponse.body.data.catalogCreate.userErrors).toEqual([]);
    const catalogId = catalogResponse.body.data.catalogCreate.catalog.id as string;

    const webPresenceResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${WEB_PRESENCE_FIELDS}
          mutation CreateResolvedWebPresence($input: WebPresenceCreateInput!) {
            webPresenceCreate(input: $input) {
              webPresence {
                ...LifecycleWebPresenceFields
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          input: {
            defaultLocale: 'de',
            alternateLocales: ['en'],
            subfolderSuffix: 'de',
          },
        },
      });

    expect(webPresenceResponse.status).toBe(200);
    expect(webPresenceResponse.body.data.webPresenceCreate.userErrors).toEqual([]);
    const webPresenceId = webPresenceResponse.body.data.webPresenceCreate.webPresence.id as string;

    const associateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation AssociateResolvedWebPresence($id: ID!, $input: MarketUpdateInput!) {
            marketUpdate(id: $id, input: $input) {
              market {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          id: marketId,
          input: {
            webPresencesToAdd: [webPresenceId],
          },
        },
      });

    expect(associateResponse.status).toBe(200);
    expect(associateResponse.body.data.marketUpdate.userErrors).toEqual([]);

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query ReadResolvedValues {
            marketsResolvedValues(buyerSignal: { countryCode: DE }) {
              currencyCode
              priceInclusivity {
                dutiesIncluded
                taxesIncluded
              }
              catalogs(first: 5) {
                nodes {
                  id
                  title
                  priceList {
                    id
                  }
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
                  defaultLocale {
                    locale
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
          }
        `,
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data.marketsResolvedValues).toEqual({
      currencyCode: 'EUR',
      priceInclusivity: {
        dutiesIncluded: true,
        taxesIncluded: true,
      },
      catalogs: {
        nodes: [
          {
            id: catalogId,
            title: 'Codex Germany Catalog',
            priceList: {
              id: 'gid://shopify/PriceList/919',
            },
          },
        ],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: catalogId,
          endCursor: catalogId,
        },
      },
      webPresences: {
        nodes: [
          {
            id: webPresenceId,
            subfolderSuffix: 'de',
            defaultLocale: {
              locale: 'de',
            },
          },
        ],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: webPresenceId,
          endCursor: webPresenceId,
        },
      },
    });

    const logResponse = await request(app).get('/__meta/log');
    expect(logResponse.body.entries.map((entry: { operationName: string }) => entry.operationName)).toEqual([
      'marketCreate',
      'catalogCreate',
      'webPresenceCreate',
      'marketUpdate',
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns MarketUserError shapes for invalid web presence inputs without staging records', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('web presence validation must not proxy'));
    const app = createApp(config).callback();

    const invalidCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation InvalidWebPresenceCreate($input: WebPresenceCreateInput!) {
            webPresenceCreate(input: $input) {
              webPresence {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          input: {
            domainId: 'gid://shopify/Domain/93049946345',
            defaultLocale: 'english',
            alternateLocales: ['en', 'en'],
            subfolderSuffix: '../ca',
          },
        },
      });

    expect(invalidCreateResponse.status).toBe(200);
    expect(invalidCreateResponse.body.data.webPresenceCreate).toEqual({
      webPresence: null,
      userErrors: [
        { field: ['input', 'domainId'], message: 'Domain does not exist', code: 'DOMAIN_NOT_FOUND' },
        { field: ['input', 'defaultLocale'], message: 'Invalid locale codes: english', code: 'INVALID' },
      ],
    });

    const invalidSuffixCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation InvalidWebPresenceSuffix($input: WebPresenceCreateInput!) {
            webPresenceCreate(input: $input) {
              webPresence {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          input: {
            defaultLocale: 'en',
            subfolderSuffix: 'ca-1',
          },
        },
      });

    expect(invalidSuffixCreateResponse.status).toBe(200);
    expect(invalidSuffixCreateResponse.body.data.webPresenceCreate).toEqual({
      webPresence: null,
      userErrors: [
        {
          field: ['input', 'subfolderSuffix'],
          message: 'Subfolder suffix must contain only letters',
          code: 'SUBFOLDER_SUFFIX_MUST_CONTAIN_ONLY_LETTERS',
        },
      ],
    });

    const unknownUpdateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UnknownWebPresenceUpdate($id: ID!, $input: WebPresenceUpdateInput!) {
            webPresenceUpdate(id: $id, input: $input) {
              webPresence {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          id: 'gid://shopify/MarketWebPresence/999999',
          input: {
            defaultLocale: 'en',
          },
        },
      });

    expect(unknownUpdateResponse.status).toBe(200);
    expect(unknownUpdateResponse.body.data.webPresenceUpdate).toEqual({
      webPresence: null,
      userErrors: [
        {
          field: ['id'],
          message: "The market web presence wasn't found.",
          code: 'WEB_PRESENCE_NOT_FOUND',
        },
      ],
    });

    const unknownDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UnknownWebPresenceDelete($id: ID!) {
            webPresenceDelete(id: $id) {
              deletedId
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          id: 'gid://shopify/MarketWebPresence/999999999999',
        },
      });

    expect(unknownDeleteResponse.status).toBe(200);
    expect(unknownDeleteResponse.body.data.webPresenceDelete).toEqual({
      deletedId: null,
      userErrors: [
        {
          field: ['id'],
          message: "The market web presence wasn't found.",
          code: 'WEB_PRESENCE_NOT_FOUND',
        },
      ],
    });

    expect(store.getLog()).toHaveLength(4);
    expect(store.getLog().map((entry) => entry.status)).toEqual(['staged', 'staged', 'staged', 'staged']);
    expect(store.getLog().map((entry) => entry.operationName)).toEqual([
      'webPresenceCreate',
      'webPresenceCreate',
      'webPresenceUpdate',
      'webPresenceDelete',
    ]);
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('stages price-list lifecycle and fixed-price mutations locally with read-after-write and meta visibility', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('price lists must not proxy'));
    const app = createApp(config).callback();

    const productResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation CreateProduct {
            productCreate(product: { title: "Contextual Pricing Hat" }) {
              product {
                id
                title
                variants(first: 1) {
                  nodes {
                    id
                    sku
                  }
                }
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
      });

    const product = productResponse.body.data.productCreate.product as {
      id: string;
      title: string;
      variants: { nodes: Array<{ id: string }> };
    };
    const variantId = product.variants.nodes[0]!.id;

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `${PRICE_LIST_FIELDS}
          mutation CreatePriceList($input: PriceListCreateInput!) {
            priceListCreate(input: $input) {
              priceList {
                ...LifecyclePriceListFields
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          input: {
            name: 'Codex EUR',
            currency: 'EUR',
            parent: {
              adjustment: {
                type: 'PERCENTAGE_DECREASE',
                value: 10,
              },
            },
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.priceListCreate.userErrors).toEqual([]);
    expect(createResponse.body.data.priceListCreate.priceList).toMatchObject({
      __typename: 'PriceList',
      id: expect.stringMatching(/^gid:\/\/shopify\/PriceList\//),
      name: 'Codex EUR',
      currency: 'EUR',
      fixedPricesCount: 0,
      parent: {
        adjustment: {
          type: 'PERCENTAGE_DECREASE',
          value: 10,
        },
      },
      prices: {
        edges: [],
      },
    });
    const priceListId = createResponse.body.data.priceListCreate.priceList.id as string;

    const addResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${PRICE_LIST_FIELDS}
          mutation AddFixedPrice($priceListId: ID!, $prices: [PriceListPriceInput!]!) {
            priceListFixedPricesAdd(priceListId: $priceListId, prices: $prices) {
              priceList {
                ...LifecyclePriceListFields
              }
              fixedPriceVariantIds
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          priceListId,
          prices: [
            {
              variantId,
              price: {
                amount: '42.0',
                currencyCode: 'EUR',
              },
              compareAtPrice: {
                amount: '50.0',
                currencyCode: 'EUR',
              },
            },
          ],
        },
      });

    expect(addResponse.status).toBe(200);
    expect(addResponse.body.data.priceListFixedPricesAdd.userErrors).toEqual([]);
    expect(addResponse.body.data.priceListFixedPricesAdd.fixedPriceVariantIds).toEqual([variantId]);
    expect(addResponse.body.data.priceListFixedPricesAdd.priceList.fixedPricesCount).toBe(1);
    expect(addResponse.body.data.priceListFixedPricesAdd.priceList.prices.edges).toEqual([
      {
        cursor: variantId,
        node: {
          price: {
            amount: '42.0',
            currencyCode: 'EUR',
          },
          compareAtPrice: {
            amount: '50.0',
            currencyCode: 'EUR',
          },
          originType: 'FIXED',
          variant: {
            id: variantId,
            sku: null,
            product: {
              id: product.id,
              title: 'Contextual Pricing Hat',
            },
          },
          quantityPriceBreaks: {
            edges: [],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: null,
              endCursor: null,
            },
          },
        },
      },
    ]);

    const duplicateAddResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DuplicateFixedPrice($priceListId: ID!, $prices: [PriceListPriceInput!]!) {
            priceListFixedPricesAdd(priceListId: $priceListId, prices: $prices) {
              priceList {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          priceListId,
          prices: [
            {
              variantId,
              price: {
                amount: '43.0',
                currencyCode: 'EUR',
              },
            },
          ],
        },
      });

    expect(duplicateAddResponse.body.data.priceListFixedPricesAdd).toEqual({
      priceList: null,
      userErrors: [
        {
          field: ['prices', 'variantId'],
          message: 'Fixed price already exists',
          code: 'TAKEN',
        },
      ],
    });

    const byProductUpdateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${PRICE_LIST_FIELDS}
          mutation ProductFixedPriceUpdate(
            $priceListId: ID!
            $pricesToAdd: [PriceListProductPriceInput!]!
            $pricesToDeleteByProductIds: [ID!]!
          ) {
            priceListFixedPricesByProductUpdate(
              priceListId: $priceListId
              pricesToAdd: $pricesToAdd
              pricesToDeleteByProductIds: $pricesToDeleteByProductIds
            ) {
              priceList {
                ...LifecyclePriceListFields
              }
              pricesToAddProducts {
                id
                title
              }
              pricesToDeleteProducts {
                id
                title
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          priceListId,
          pricesToAdd: [
            {
              productId: product.id,
              price: {
                amount: '40.0',
                currencyCode: 'EUR',
              },
            },
          ],
          pricesToDeleteByProductIds: [],
        },
      });

    expect(byProductUpdateResponse.body.data.priceListFixedPricesByProductUpdate.userErrors).toEqual([]);
    expect(byProductUpdateResponse.body.data.priceListFixedPricesByProductUpdate.pricesToAddProducts).toEqual([
      {
        id: product.id,
        title: product.title,
      },
    ]);
    expect(byProductUpdateResponse.body.data.priceListFixedPricesByProductUpdate.pricesToDeleteProducts).toEqual([]);
    expect(
      byProductUpdateResponse.body.data.priceListFixedPricesByProductUpdate.priceList.prices.edges[0].node.price,
    ).toEqual({
      amount: '40.0',
      currencyCode: 'EUR',
    });

    const missingProductResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation ProductFixedPriceMissingProduct(
            $priceListId: ID!
            $pricesToAdd: [PriceListProductPriceInput!]!
            $pricesToDeleteByProductIds: [ID!]!
          ) {
            priceListFixedPricesByProductUpdate(
              priceListId: $priceListId
              pricesToAdd: $pricesToAdd
              pricesToDeleteByProductIds: $pricesToDeleteByProductIds
            ) {
              priceList {
                id
              }
              pricesToAddProducts {
                id
              }
              pricesToDeleteProducts {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          priceListId,
          pricesToAdd: [
            {
              productId: 'gid://shopify/Product/0',
              price: {
                amount: '41.0',
                currencyCode: 'EUR',
              },
            },
          ],
          pricesToDeleteByProductIds: [],
        },
      });

    expect(missingProductResponse.body.data.priceListFixedPricesByProductUpdate).toEqual({
      priceList: null,
      pricesToAddProducts: null,
      pricesToDeleteProducts: null,
      userErrors: [
        {
          field: ['pricesToAdd', '0', 'productId'],
          message: 'Product gid://shopify/Product/0 in `pricesToAdd` does not exist.',
          code: 'PRODUCT_DOES_NOT_EXIST',
        },
      ],
    });

    const readAfterMissingProductResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${PRICE_LIST_FIELDS}
          query ReadAfterMissingProduct($id: ID!) {
            priceList(id: $id) {
              ...LifecyclePriceListFields
            }
          }
        `,
        variables: {
          id: priceListId,
        },
      });

    expect(readAfterMissingProductResponse.body.data.priceList.prices.edges).toHaveLength(1);
    expect(readAfterMissingProductResponse.body.data.priceList.prices.edges[0].node).toMatchObject({
      price: {
        amount: '40.0',
        currencyCode: 'EUR',
      },
      variant: {
        id: variantId,
      },
    });

    const quantityRulesAddResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${PRICE_LIST_FIELDS}
          mutation AddQuantityRule($priceListId: ID!, $quantityRules: [QuantityRuleInput!]!) {
            quantityRulesAdd(priceListId: $priceListId, quantityRules: $quantityRules) {
              quantityRules {
                minimum
                maximum
                increment
                isDefault
                originType
                productVariant {
                  id
                }
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          priceListId,
          quantityRules: [
            {
              variantId,
              minimum: 2,
              maximum: 10,
              increment: 2,
            },
          ],
        },
      });

    expect(quantityRulesAddResponse.body.data.quantityRulesAdd).toEqual({
      quantityRules: [
        {
          minimum: 2,
          maximum: 10,
          increment: 2,
          isDefault: false,
          originType: 'FIXED',
          productVariant: {
            id: variantId,
          },
        },
      ],
      userErrors: [],
    });

    const quantityPricingResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation QuantityPricing($priceListId: ID!, $input: QuantityPricingByVariantUpdateInput!) {
            quantityPricingByVariantUpdate(priceListId: $priceListId, input: $input) {
              productVariants {
                id
                title
                sku
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          priceListId,
          input: {
            pricesToAdd: [
              {
                variantId,
                price: {
                  amount: '38.0',
                  currencyCode: 'EUR',
                },
              },
            ],
            pricesToDeleteByVariantId: [],
            quantityRulesToAdd: [
              {
                variantId,
                minimum: 4,
                maximum: 12,
                increment: 4,
              },
            ],
            quantityRulesToDeleteByVariantId: [],
            quantityPriceBreaksToAdd: [
              {
                variantId,
                minimumQuantity: 8,
                price: {
                  amount: '35.0',
                  currencyCode: 'EUR',
                },
              },
            ],
            quantityPriceBreaksToDelete: [],
          },
        },
      });

    expect(quantityPricingResponse.body.data.quantityPricingByVariantUpdate).toEqual({
      productVariants: [
        {
          id: variantId,
          title: 'Default Title',
          sku: null,
        },
      ],
      userErrors: [],
    });

    const readQuantityPricingResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${PRICE_LIST_FIELDS}
          query ReadQuantityPricing($id: ID!) {
            priceList(id: $id) {
              ...LifecyclePriceListFields
            }
          }
        `,
        variables: {
          id: priceListId,
        },
      });

    expect(readQuantityPricingResponse.body.data.priceList.quantityRules.edges).toEqual([
      {
        cursor: variantId,
        node: {
          minimum: 4,
          maximum: 12,
          increment: 4,
          isDefault: false,
          originType: 'FIXED',
          productVariant: {
            id: variantId,
          },
        },
      },
    ]);
    expect(readQuantityPricingResponse.body.data.priceList.prices.edges).toHaveLength(1);
    expect(readQuantityPricingResponse.body.data.priceList.prices.edges[0].node.price).toEqual({
      amount: '38.0',
      currencyCode: 'EUR',
    });
    expect(readQuantityPricingResponse.body.data.priceList.prices.edges[0].node.quantityPriceBreaks.edges).toEqual([
      {
        cursor: expect.stringMatching(/^gid:\/\/shopify\/QuantityPriceBreak\//),
        node: {
          id: expect.stringMatching(/^gid:\/\/shopify\/QuantityPriceBreak\//),
          minimumQuantity: 8,
          price: {
            amount: '35.0',
            currencyCode: 'EUR',
          },
          variant: {
            id: variantId,
          },
        },
      },
    ]);

    const quantityRulesDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeleteQuantityRule($priceListId: ID!, $variantIds: [ID!]!) {
            quantityRulesDelete(priceListId: $priceListId, variantIds: $variantIds) {
              deletedQuantityRulesVariantIds
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          priceListId,
          variantIds: [variantId],
        },
      });

    expect(quantityRulesDeleteResponse.body.data.quantityRulesDelete).toEqual({
      deletedQuantityRulesVariantIds: [variantId],
      userErrors: [],
    });

    const deleteFixedResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${PRICE_LIST_FIELDS}
          mutation DeleteFixedPrice($priceListId: ID!, $variantIds: [ID!]!) {
            priceListFixedPricesDelete(priceListId: $priceListId, variantIds: $variantIds) {
              priceList {
                ...LifecyclePriceListFields
              }
              deletedFixedPriceVariantIds
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          priceListId,
          variantIds: [variantId],
        },
      });

    expect(deleteFixedResponse.body.data.priceListFixedPricesDelete).toMatchObject({
      deletedFixedPriceVariantIds: [variantId],
      priceList: {
        fixedPricesCount: 0,
        prices: {
          edges: [],
        },
      },
      userErrors: [],
    });

    await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation AddFixedPriceAgain($priceListId: ID!, $prices: [PriceListPriceInput!]!) {
            priceListFixedPricesAdd(priceListId: $priceListId, prices: $prices) {
              priceList {
                id
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          priceListId,
          prices: [
            {
              variantId,
              price: {
                amount: '39.0',
                currencyCode: 'EUR',
              },
            },
          ],
        },
      });

    const currencyUpdateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${PRICE_LIST_FIELDS}
          mutation ChangeCurrency($id: ID!, $input: PriceListUpdateInput!) {
            priceListUpdate(id: $id, input: $input) {
              priceList {
                ...LifecyclePriceListFields
              }
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          id: priceListId,
          input: {
            currency: 'CAD',
          },
        },
      });

    expect(currencyUpdateResponse.body.data.priceListUpdate.userErrors).toEqual([]);
    expect(currencyUpdateResponse.body.data.priceListUpdate.priceList).toMatchObject({
      id: priceListId,
      currency: 'CAD',
      fixedPricesCount: 0,
      prices: {
        edges: [],
      },
    });

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `${PRICE_LIST_FIELDS}
          query ReadPriceList($id: ID!) {
            priceList(id: $id) {
              ...LifecyclePriceListFields
            }
            priceLists(first: 10) {
              nodes {
                id
                name
                currency
                fixedPricesCount
              }
            }
          }
        `,
        variables: {
          id: priceListId,
        },
      });

    expect(readResponse.body.data.priceList).toMatchObject({
      id: priceListId,
      name: 'Codex EUR',
      currency: 'CAD',
      fixedPricesCount: 0,
    });
    expect(readResponse.body.data.priceLists.nodes).toContainEqual({
      id: priceListId,
      name: 'Codex EUR',
      currency: 'CAD',
      fixedPricesCount: 0,
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DeletePriceList($id: ID!) {
            priceListDelete(id: $id) {
              deletedId
              userErrors {
                field
                message
                code
              }
            }
          }
        `,
        variables: {
          id: priceListId,
        },
      });

    expect(deleteResponse.body.data.priceListDelete).toEqual({
      deletedId: priceListId,
      userErrors: [],
    });

    const readAfterDeleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query ReadDeletedPriceList($id: ID!) {
            priceList(id: $id) {
              id
            }
            priceLists(first: 10) {
              nodes {
                id
              }
            }
          }
        `,
        variables: {
          id: priceListId,
        },
      });

    expect(readAfterDeleteResponse.body.data.priceList).toBeNull();
    expect(readAfterDeleteResponse.body.data.priceLists.nodes).not.toContainEqual({ id: priceListId });

    const stateResponse = await request(app).get('/__meta/state');
    const logResponse = await request(app).get('/__meta/log');
    expect(stateResponse.body.stagedState.deletedPriceListIds).toEqual({ [priceListId]: true });
    expect(logResponse.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
      'staged',
    ]);
    expect(logResponse.body.entries[1]).toMatchObject({
      operationName: 'priceListCreate',
      requestBody: {
        variables: {
          input: {
            name: 'Codex EUR',
            currency: 'EUR',
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
