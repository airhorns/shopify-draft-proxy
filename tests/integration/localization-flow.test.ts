import { createHash } from 'node:crypto';

import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { ProductMetafieldRecord, ProductRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const productId = 'gid://shopify/Product/314001';
const metafieldId = 'gid://shopify/Metafield/314001';

function digest(value: string): string {
  return createHash('sha256').update(value).digest('hex');
}

function makeProduct(): ProductRecord {
  return {
    id: productId,
    legacyResourceId: '314001',
    title: 'Localization Snowboard',
    handle: 'localization-snowboard',
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

function makeMetafield(): ProductMetafieldRecord {
  return {
    id: metafieldId,
    productId,
    namespace: 'custom',
    key: 'material',
    type: 'single_line_text_field',
    value: 'Maple',
    compareDigest: digest('Maple'),
    jsonValue: 'Maple',
    createdAt: '2026-04-01T00:00:00.000Z',
    updatedAt: '2026-04-01T00:00:00.000Z',
    ownerType: 'PRODUCT',
  };
}

function seedLocalizationState(): void {
  store.replaceBaseAvailableLocales([
    { isoCode: 'en', name: 'English' },
    { isoCode: 'fr', name: 'French' },
  ]);
  store.upsertBaseShopLocales([
    { locale: 'en', name: 'English', primary: true, published: true, marketWebPresenceIds: [] },
  ]);
  store.upsertBaseProducts([makeProduct()]);
  store.replaceBaseMetafieldsForProduct(productId, [makeMetafield()]);
}

describe('Localization staging', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves locale and translatable-resource reads locally in snapshot mode', async () => {
    seedLocalizationState();
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('localization reads must not proxy in snapshot'));
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query LocalizationReads($productId: ID!, $ids: [ID!]!) {
            availableLocales {
              isoCode
              name
            }
            allShopLocales: shopLocales {
              locale
              name
              primary
              published
            }
            product: translatableResource(resourceId: $productId) {
              resourceId
              translatableContent {
                key
                value
                digest
                locale
                type
              }
              translations(locale: "fr") {
                key
              }
              nestedTranslatableResources(first: 2) {
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
            resources: translatableResources(first: 1, resourceType: PRODUCT) {
              nodes {
                resourceId
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
            byIds: translatableResourcesByIds(first: 5, resourceIds: $ids) {
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
          productId,
          ids: [metafieldId, 'gid://shopify/Product/404'],
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      availableLocales: [
        { isoCode: 'en', name: 'English' },
        { isoCode: 'fr', name: 'French' },
      ],
      allShopLocales: [{ locale: 'en', name: 'English', primary: true, published: true }],
      product: {
        resourceId: productId,
        translatableContent: [
          {
            key: 'title',
            value: 'Localization Snowboard',
            digest: digest('Localization Snowboard'),
            locale: 'en',
            type: 'SINGLE_LINE_TEXT_FIELD',
          },
          {
            key: 'handle',
            value: 'localization-snowboard',
            digest: digest('localization-snowboard'),
            locale: 'en',
            type: 'URI',
          },
          {
            key: 'body_html',
            value: '<p>Fast board</p>',
            digest: digest('<p>Fast board</p>'),
            locale: 'en',
            type: 'HTML',
          },
          {
            key: 'product_type',
            value: 'Snowboard',
            digest: digest('Snowboard'),
            locale: 'en',
            type: 'SINGLE_LINE_TEXT_FIELD',
          },
        ],
        translations: [],
        nestedTranslatableResources: {
          nodes: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
      },
      resources: {
        nodes: [{ resourceId: productId }],
        edges: [{ cursor: `cursor:${productId}`, node: { resourceId: productId } }],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: `cursor:${productId}`,
          endCursor: `cursor:${productId}`,
        },
      },
      byIds: {
        nodes: [{ resourceId: metafieldId }],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: `cursor:${metafieldId}`,
          endCursor: `cursor:${metafieldId}`,
        },
      },
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('stages shop locale and product translation mutations with downstream read-after-write', async () => {
    seedLocalizationState();
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('localization mutations must not proxy'));
    const app = createApp(config).callback();

    const enableResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation EnableLocale($locale: String!) {
            shopLocaleEnable(locale: $locale) {
              shopLocale {
                locale
                name
                primary
                published
              }
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { locale: 'fr' },
      });

    expect(enableResponse.body.data.shopLocaleEnable).toEqual({
      shopLocale: { locale: 'fr', name: 'French', primary: false, published: false },
      userErrors: [],
    });

    const registerResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation RegisterTranslation($resourceId: ID!, $translations: [TranslationInput!]!) {
            translationsRegister(resourceId: $resourceId, translations: $translations) {
              translations {
                key
                value
                locale
                outdated
                updatedAt
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
          resourceId: productId,
          translations: [
            {
              locale: 'fr',
              key: 'title',
              value: 'Planche localisee',
              translatableContentDigest: digest('Localization Snowboard'),
            },
          ],
        },
      });

    expect(registerResponse.status).toBe(200);
    expect(registerResponse.body.data.translationsRegister.translations).toMatchObject([
      {
        key: 'title',
        value: 'Planche localisee',
        locale: 'fr',
        outdated: false,
        market: null,
      },
    ]);
    expect(registerResponse.body.data.translationsRegister.translations[0].updatedAt).toEqual(expect.any(String));
    expect(registerResponse.body.data.translationsRegister.userErrors).toEqual([]);

    const downstreamResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query Downstream($resourceId: ID!) {
            translatableResource(resourceId: $resourceId) {
              resourceId
              translations(locale: "fr") {
                key
                value
                locale
                outdated
              }
            }
            shopLocales(published: false) {
              locale
              published
            }
          }
        `,
        variables: { resourceId: productId },
      });

    expect(downstreamResponse.body.data).toEqual({
      translatableResource: {
        resourceId: productId,
        translations: [{ key: 'title', value: 'Planche localisee', locale: 'fr', outdated: false }],
      },
      shopLocales: [{ locale: 'fr', published: false }],
    });

    const removeResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation RemoveTranslation($resourceId: ID!) {
            translationsRemove(resourceId: $resourceId, translationKeys: ["title"], locales: ["fr"]) {
              translations {
                key
                value
                locale
              }
              userErrors {
                field
                message
                code
              }
            }
            shopLocaleDisable(locale: "fr") {
              locale
              userErrors {
                field
                message
              }
            }
          }
        `,
        variables: { resourceId: productId },
      });

    expect(removeResponse.body.data.translationsRemove).toMatchObject({
      translations: [{ key: 'title', value: 'Planche localisee', locale: 'fr' }],
      userErrors: [],
    });
    expect(removeResponse.body.data.shopLocaleDisable).toEqual({ locale: 'fr', userErrors: [] });

    const afterResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query After($resourceId: ID!) {
            translatableResource(resourceId: $resourceId) {
              translations(locale: "fr") {
                key
              }
            }
            shopLocales {
              locale
            }
          }
        `,
        variables: { resourceId: productId },
      });

    expect(afterResponse.body.data).toEqual({
      translatableResource: { translations: [] },
      shopLocales: [{ locale: 'en' }],
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
    expect(store.getLog().map((entry) => entry.status)).toEqual(['staged', 'staged', 'staged']);
  });

  it('keeps unsupported resource and owner branches explicit', async () => {
    seedLocalizationState();
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(
      new Error('unsupported localization branches must not proxy in snapshot'),
    );
    const app = createApp(config).callback();

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          query UnsupportedResources {
            unsupportedResources: translatableResources(first: 2, resourceType: PRODUCT_OPTION) {
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
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data).toEqual({
      unsupportedResources: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
    });

    const mutationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `#graphql
          mutation UnsupportedMutation($resourceId: ID!, $translations: [TranslationInput!]!) {
            missing: translationsRegister(resourceId: $resourceId, translations: $translations) {
              translations {
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
          resourceId: 'gid://shopify/Product/404',
          translations: [
            {
              locale: 'fr',
              key: 'title',
              value: 'Nope',
              translatableContentDigest: digest('Nope'),
            },
          ],
        },
      });

    expect(mutationResponse.status).toBe(200);
    expect(mutationResponse.body.data).toEqual({
      missing: {
        translations: null,
        userErrors: [
          {
            field: ['resourceId'],
            message: 'Resource gid://shopify/Product/404 does not exist',
            code: 'RESOURCE_NOT_FOUND',
          },
        ],
      },
    });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });
});
