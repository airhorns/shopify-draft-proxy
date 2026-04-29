import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';
import type { ProductMetafieldRecord, ProductRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const productId = 'gid://shopify/Product/182001';
const marketId = 'gid://shopify/Market/182001';
const secondaryMarketId = 'gid://shopify/Market/182002';
const materialMetafieldId = 'gid://shopify/Metafield/182001';
const careMetafieldId = 'gid://shopify/Metafield/182002';

function makeProduct(): ProductRecord {
  return {
    id: productId,
    legacyResourceId: '182001',
    title: 'Localized Snowboard',
    handle: 'localized-snowboard',
    status: 'ACTIVE',
    publicationIds: [],
    createdAt: '2026-04-01T00:00:00.000Z',
    updatedAt: '2026-04-01T00:00:00.000Z',
    vendor: 'Hermes',
    productType: 'Snowboard',
    tags: [],
    totalInventory: 3,
    tracksInventory: true,
    descriptionHtml: '<p>Fast board</p>',
    onlineStorePreviewUrl: null,
    templateSuffix: null,
    seo: {
      title: null,
      description: null,
    },
    category: null,
  };
}

function makeMetafield(id: string, key: string, value: string, compareDigest: string): ProductMetafieldRecord {
  return {
    id,
    productId,
    namespace: 'custom',
    key,
    type: 'single_line_text_field',
    value,
    compareDigest,
    jsonValue: value,
    createdAt: '2026-04-01T00:00:00.000Z',
    updatedAt: '2026-04-01T00:00:00.000Z',
    ownerType: 'PRODUCT',
    marketLocalizableContent: [{ key: 'value', value, digest: compareDigest }],
  };
}

function seedMarketLocalizationState(): void {
  store.upsertBaseProducts([makeProduct()]);
  store.replaceBaseMetafieldsForProduct(productId, [
    makeMetafield(materialMetafieldId, 'material', 'Maple', 'digest-material'),
    makeMetafield(careMetafieldId, 'care', 'Wax weekly', 'digest-care'),
  ]);
  store.upsertBaseMarkets([
    {
      id: marketId,
      name: 'Canada',
    },
    {
      id: secondaryMarketId,
      name: 'International',
    },
  ]);
}

describe('Markets localization staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves product metafield market-localizable read roots with filters and pagination', async () => {
    seedMarketLocalizationState();
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('market-localizable reads must not proxy in snapshot'));
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query MarketLocalizableReads(
            $resourceId: ID!
            $resourceIds: [ID!]!
            $first: Int
            $after: String
            $marketId: ID!
          ) {
            resource: marketLocalizableResource(resourceId: $resourceId) {
              resourceId
              marketLocalizableContent {
                key
                value
                digest
              }
              marketLocalizations(marketId: $marketId) {
                key
              }
            }
            firstPage: marketLocalizableResources(first: $first, resourceType: METAFIELD) {
              nodes {
                resourceId
                marketLocalizableContent {
                  key
                  value
                  digest
                }
              }
              edges {
                cursor
                node {
                  resourceId
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            afterPage: marketLocalizableResources(first: $first, after: $after, resourceType: METAFIELD) {
              nodes {
                resourceId
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            byIds: marketLocalizableResourcesByIds(first: 5, resourceIds: $resourceIds) {
              nodes {
                resourceId
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
          resourceId: materialMetafieldId,
          resourceIds: [careMetafieldId, 'gid://shopify/Metafield/404'],
          first: 1,
          after: `cursor:${materialMetafieldId}`,
          marketId,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      resource: {
        resourceId: materialMetafieldId,
        marketLocalizableContent: [{ key: 'value', value: 'Maple', digest: 'digest-material' }],
        marketLocalizations: [],
      },
      firstPage: {
        nodes: [
          {
            resourceId: materialMetafieldId,
            marketLocalizableContent: [{ key: 'value', value: 'Maple', digest: 'digest-material' }],
          },
        ],
        edges: [{ cursor: `cursor:${materialMetafieldId}`, node: { resourceId: materialMetafieldId } }],
        pageInfo: {
          hasNextPage: true,
          hasPreviousPage: false,
          startCursor: `cursor:${materialMetafieldId}`,
          endCursor: `cursor:${materialMetafieldId}`,
        },
      },
      afterPage: {
        nodes: [{ resourceId: careMetafieldId }],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: true,
          startCursor: `cursor:${careMetafieldId}`,
          endCursor: `cursor:${careMetafieldId}`,
        },
      },
      byIds: {
        nodes: [{ resourceId: careMetafieldId }],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: `cursor:${careMetafieldId}`,
          endCursor: `cursor:${careMetafieldId}`,
        },
      },
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('stages marketLocalizationsRegister and marketLocalizationsRemove locally with read-after-write', async () => {
    seedMarketLocalizationState();
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('market localization mutations must not proxy'));
    const app = createApp(config).callback();

    const registerResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation RegisterLocalization(
            $resourceId: ID!
            $marketLocalizations: [MarketLocalizationRegisterInput!]!
          ) {
            marketLocalizationsRegister(resourceId: $resourceId, marketLocalizations: $marketLocalizations) {
              marketLocalizations {
                key
                value
                updatedAt
                outdated
                market {
                  id
                  name
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
          resourceId: materialMetafieldId,
          marketLocalizations: [
            {
              key: 'value',
              value: 'Erable',
              marketId,
              marketLocalizableContentDigest: 'digest-material',
            },
          ],
        },
      });

    expect(registerResponse.status).toBe(200);
    expect(registerResponse.body.data.marketLocalizationsRegister).toEqual({
      marketLocalizations: [
        {
          key: 'value',
          value: 'Erable',
          updatedAt: '2024-01-01T00:00:00.000Z',
          outdated: false,
          market: {
            id: marketId,
            name: 'Canada',
          },
        },
      ],
      userErrors: [],
    });

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UpdateLocalization($resourceId: ID!, $marketLocalizations: [MarketLocalizationRegisterInput!]!) {
            marketLocalizationsRegister(resourceId: $resourceId, marketLocalizations: $marketLocalizations) {
              marketLocalizations {
                key
                value
                updatedAt
                outdated
                market {
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
          resourceId: materialMetafieldId,
          marketLocalizations: [
            {
              key: 'value',
              value: 'Erable canadien',
              marketId,
              marketLocalizableContentDigest: 'digest-material',
            },
          ],
        },
      });

    expect(updateResponse.status).toBe(200);
    expect(updateResponse.body.data.marketLocalizationsRegister).toEqual({
      marketLocalizations: [
        {
          key: 'value',
          value: 'Erable canadien',
          updatedAt: '2024-01-01T00:00:02.000Z',
          outdated: false,
          market: {
            id: marketId,
          },
        },
      ],
      userErrors: [],
    });

    const readAfterRegisterResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query ReadLocalization($resourceId: ID!, $marketId: ID!) {
            marketLocalizableResource(resourceId: $resourceId) {
              resourceId
              marketLocalizations(marketId: $marketId) {
                key
                value
                updatedAt
                outdated
                market {
                  id
                  name
                }
              }
            }
          }
        `,
        variables: {
          resourceId: materialMetafieldId,
          marketId,
        },
      });

    expect(readAfterRegisterResponse.status).toBe(200);
    expect(readAfterRegisterResponse.body.data.marketLocalizableResource.marketLocalizations).toEqual([
      {
        key: 'value',
        value: 'Erable canadien',
        updatedAt: '2024-01-01T00:00:02.000Z',
        outdated: false,
        market: {
          id: marketId,
          name: 'Canada',
        },
      },
    ]);

    const removeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation RemoveLocalization($resourceId: ID!, $keys: [String!]!, $marketIds: [ID!]!) {
            marketLocalizationsRemove(
              resourceId: $resourceId
              marketLocalizationKeys: $keys
              marketIds: $marketIds
            ) {
              marketLocalizations {
                key
                value
                market {
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
          resourceId: materialMetafieldId,
          keys: ['value'],
          marketIds: [marketId],
        },
      });

    expect(removeResponse.status).toBe(200);
    expect(removeResponse.body.data.marketLocalizationsRemove).toEqual({
      marketLocalizations: [
        {
          key: 'value',
          value: 'Erable canadien',
          market: {
            id: marketId,
          },
        },
      ],
      userErrors: [],
    });

    const readAfterRemoveResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query ReadRemovedLocalization($resourceId: ID!, $marketId: ID!) {
            marketLocalizableResource(resourceId: $resourceId) {
              marketLocalizations(marketId: $marketId) {
                key
              }
            }
          }
        `,
        variables: {
          resourceId: materialMetafieldId,
          marketId,
        },
      });

    expect(readAfterRemoveResponse.status).toBe(200);
    expect(readAfterRemoveResponse.body.data.marketLocalizableResource.marketLocalizations).toEqual([]);

    const logResponse = await request(app).get('/__meta/log');
    expect(
      logResponse.body.entries.map((entry: { operationName: string; status: string }) => ({
        operationName: entry.operationName,
        status: entry.status,
      })),
    ).toEqual([
      { operationName: 'marketLocalizationsRegister', status: 'staged' },
      { operationName: 'marketLocalizationsRegister', status: 'staged' },
      { operationName: 'marketLocalizationsRemove', status: 'staged' },
    ]);
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('returns TranslationUserError shapes for localization validation failures without staging', async () => {
    seedMarketLocalizationState();
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('market localization validation must not proxy'));
    const app = createApp(config).callback();

    const invalidResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation InvalidLocalization($resourceId: ID!, $marketLocalizations: [MarketLocalizationRegisterInput!]!) {
            marketLocalizationsRegister(resourceId: $resourceId, marketLocalizations: $marketLocalizations) {
              marketLocalizations {
                key
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
          resourceId: materialMetafieldId,
          marketLocalizations: [
            {
              key: 'title',
              value: '',
              marketId: 'gid://shopify/Market/404',
              marketLocalizableContentDigest: 'wrong-digest',
            },
          ],
        },
      });

    expect(invalidResponse.status).toBe(200);
    expect(invalidResponse.body.data.marketLocalizationsRegister).toEqual({
      marketLocalizations: null,
      userErrors: [
        {
          field: ['marketLocalizations', '0', 'marketId'],
          message: 'Market gid://shopify/Market/404 does not exist',
          code: 'MARKET_DOES_NOT_EXIST',
        },
        {
          field: ['marketLocalizations', '0', 'key'],
          message: 'Key title is not market localizable for this resource',
          code: 'INVALID_KEY_FOR_MODEL',
        },
        {
          field: ['marketLocalizations', '0', 'value'],
          message: "Value can't be blank",
          code: 'BLANK',
        },
      ],
    });

    const digestMismatchResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation DigestMismatch($resourceId: ID!, $marketLocalizations: [MarketLocalizationRegisterInput!]!) {
            marketLocalizationsRegister(resourceId: $resourceId, marketLocalizations: $marketLocalizations) {
              marketLocalizations {
                key
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
          resourceId: materialMetafieldId,
          marketLocalizations: [
            {
              key: 'value',
              value: 'Erable',
              marketId,
              marketLocalizableContentDigest: 'wrong-digest',
            },
          ],
        },
      });

    expect(digestMismatchResponse.status).toBe(200);
    expect(digestMismatchResponse.body.data.marketLocalizationsRegister).toEqual({
      marketLocalizations: null,
      userErrors: [
        {
          field: ['marketLocalizations', '0', 'marketLocalizableContentDigest'],
          message: 'Market localizable content digest does not match the resource content',
          code: 'INVALID_MARKET_LOCALIZABLE_CONTENT',
        },
      ],
    });

    const missingInputResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation MissingLocalizationInput($resourceId: ID!, $marketLocalizations: [MarketLocalizationRegisterInput!]!) {
            marketLocalizationsRegister(resourceId: $resourceId, marketLocalizations: $marketLocalizations) {
              marketLocalizations {
                key
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
          resourceId: materialMetafieldId,
          marketLocalizations: [],
        },
      });

    expect(missingInputResponse.status).toBe(200);
    expect(missingInputResponse.body.data.marketLocalizationsRegister).toEqual({
      marketLocalizations: null,
      userErrors: [
        {
          field: ['marketLocalizations'],
          message: 'At least one market localization is required',
          code: 'BLANK',
        },
      ],
    });

    const unknownResourceResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UnknownResource($resourceId: ID!, $marketLocalizations: [MarketLocalizationRegisterInput!]!) {
            marketLocalizationsRegister(resourceId: $resourceId, marketLocalizations: $marketLocalizations) {
              marketLocalizations {
                key
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
          resourceId: 'gid://shopify/Metafield/404',
          marketLocalizations: [
            {
              key: 'value',
              value: 'Nope',
              marketId,
              marketLocalizableContentDigest: 'digest-material',
            },
          ],
        },
      });

    expect(unknownResourceResponse.status).toBe(200);
    expect(unknownResourceResponse.body.data.marketLocalizationsRegister).toEqual({
      marketLocalizations: null,
      userErrors: [
        {
          field: ['resourceId'],
          message: 'Resource gid://shopify/Metafield/404 does not exist',
          code: 'RESOURCE_NOT_FOUND',
        },
      ],
    });

    const removeInvalidResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation InvalidRemove($resourceId: ID!, $keys: [String!]!, $marketIds: [ID!]!) {
            marketLocalizationsRemove(
              resourceId: $resourceId
              marketLocalizationKeys: $keys
              marketIds: $marketIds
            ) {
              marketLocalizations {
                key
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
          resourceId: materialMetafieldId,
          keys: ['title'],
          marketIds: ['gid://shopify/Market/404'],
        },
      });

    expect(removeInvalidResponse.status).toBe(200);
    expect(removeInvalidResponse.body.data.marketLocalizationsRemove).toEqual({
      marketLocalizations: null,
      userErrors: [
        {
          field: ['marketLocalizationKeys', '0'],
          message: 'Key title is not market localizable for this resource',
          code: 'INVALID_KEY_FOR_MODEL',
        },
        {
          field: ['marketIds', '0'],
          message: 'Market gid://shopify/Market/404 does not exist',
          code: 'MARKET_DOES_NOT_EXIST',
        },
      ],
    });

    const stateResponse = await request(app).get('/__meta/state');
    expect(stateResponse.body.stagedState.marketLocalizations).toEqual({});
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });
});
