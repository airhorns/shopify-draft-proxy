import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

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
});
