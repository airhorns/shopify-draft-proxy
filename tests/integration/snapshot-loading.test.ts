import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

import request from 'supertest';
import { beforeEach, afterEach, describe, expect, it } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';

describe('snapshot loading', () => {
  let tempDir: string;

  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    tempDir = mkdtempSync(path.join(tmpdir(), 'shopify-draft-proxy-snapshot-'));
  });

  afterEach(() => {
    rmSync(tempDir, { recursive: true, force: true });
  });

  it('loads normalized snapshot state from snapshotPath and restores it on meta reset', async () => {
    const snapshotPath = path.join(tempDir, 'normalized-snapshot.json');
    writeFileSync(
      snapshotPath,
      JSON.stringify(
        {
          kind: 'normalized-state-snapshot',
          baseState: {
            products: {
              'gid://shopify/Product/9001': {
                id: 'gid://shopify/Product/9001',
                legacyResourceId: '9001',
                title: 'Snapshot Product',
                handle: 'snapshot-product',
                status: 'ACTIVE',
                publicationIds: [],
                createdAt: '2025-01-02T00:00:00.000Z',
                updatedAt: '2025-01-03T00:00:00.000Z',
                vendor: 'HERMES',
                productType: 'SNAPSHOT',
                tags: ['snapshot'],
                totalInventory: 4,
                tracksInventory: true,
                descriptionHtml: '<p>Snapshot product description</p>',
                onlineStorePreviewUrl: null,
                templateSuffix: null,
                seo: {
                  title: 'Snapshot Product SEO',
                  description: 'Snapshot product SEO description',
                },
                category: null,
              },
            },
            productVariants: {},
            productOptions: {},
            collections: {},
            customers: {
              'gid://shopify/Customer/7001': {
                id: 'gid://shopify/Customer/7001',
                firstName: 'Ada',
                lastName: 'Snapshot',
                displayName: 'Ada Snapshot',
                email: 'ada.snapshot@example.com',
                legacyResourceId: '7001',
                locale: 'en',
                note: 'Snapshot customer note',
                canDelete: true,
                verifiedEmail: true,
                taxExempt: false,
                state: 'ENABLED',
                tags: ['snapshot', 'vip'],
                numberOfOrders: '3',
                amountSpent: {
                  amount: '17.50',
                  currencyCode: 'USD',
                },
                defaultEmailAddress: {
                  emailAddress: 'ada.snapshot@example.com',
                },
                defaultPhoneNumber: {
                  phoneNumber: '+141****7001',
                },
                defaultAddress: {
                  address1: '1 Snapshot Way',
                  city: 'Testville',
                  province: 'CA',
                  country: 'United States',
                  zip: '94107',
                  formattedArea: 'Testville, CA',
                },
                createdAt: '2025-01-04T00:00:00.000Z',
                updatedAt: '2025-01-05T00:00:00.000Z',
              },
            },
            productCollections: {},
            productMedia: {},
            productMetafields: {},
            deletedProductIds: {},
            deletedCollectionIds: {},
            deletedCustomerIds: {},
          },
          customerCatalogConnection: {
            orderedCustomerIds: ['gid://shopify/Customer/7001'],
            cursorByCustomerId: {
              'gid://shopify/Customer/7001': 'opaque-snapshot-customer-cursor',
            },
            pageInfo: {
              hasNextPage: true,
              hasPreviousPage: false,
              startCursor: 'opaque-snapshot-customer-cursor',
              endCursor: 'opaque-snapshot-customer-cursor',
            },
          },
          customerSearchConnections: {
            '{"query":"state:ENABLED","sortKey":"UPDATED_AT","reverse":true}': {
              orderedCustomerIds: ['gid://shopify/Customer/7001'],
              cursorByCustomerId: {
                'gid://shopify/Customer/7001': 'opaque-search-cursor',
              },
              pageInfo: {
                hasNextPage: false,
                hasPreviousPage: false,
                startCursor: 'opaque-search-cursor',
                endCursor: 'opaque-search-cursor',
              },
            },
          },
        },
        null,
        2,
      ),
    );

    const appConfig: AppConfig = {
      port: 3000,
      shopifyAdminOrigin: 'https://example.myshopify.com',
      readMode: 'snapshot',
      snapshotPath,
    };

    const app = createApp(appConfig).callback();

    const productResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query SnapshotProduct($id: ID!) { product(id: $id) { id title handle status vendor productType tags descriptionHtml seo { title description } } }',
        variables: {
          id: 'gid://shopify/Product/9001',
        },
      });

    expect(productResponse.status).toBe(200);
    expect(productResponse.body.data.product).toEqual({
      id: 'gid://shopify/Product/9001',
      title: 'Snapshot Product',
      handle: 'snapshot-product',
      status: 'ACTIVE',
      vendor: 'HERMES',
      productType: 'SNAPSHOT',
      tags: ['snapshot'],
      descriptionHtml: '<p>Snapshot product description</p>',
      seo: {
        title: 'Snapshot Product SEO',
        description: 'Snapshot product SEO description',
      },
    });

    const customersResponse = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query SnapshotCustomers($query: String!) { customers(first: 1, query: $query, sortKey: UPDATED_AT, reverse: true) { edges { cursor node { id displayName email note tags numberOfOrders defaultAddress { city formattedArea } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }',
        variables: {
          query: 'state:ENABLED',
        },
      });

    expect(customersResponse.status).toBe(200);
    expect(customersResponse.body.data.customers).toEqual({
      edges: [
        {
          cursor: 'opaque-snapshot-customer-cursor',
          node: {
            id: 'gid://shopify/Customer/7001',
            displayName: 'Ada Snapshot',
            email: 'ada.snapshot@example.com',
            note: 'Snapshot customer note',
            tags: ['snapshot', 'vip'],
            numberOfOrders: '3',
            defaultAddress: {
              city: 'Testville',
              formattedArea: 'Testville, CA',
            },
          },
        },
      ],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: 'opaque-snapshot-customer-cursor',
        endCursor: 'opaque-snapshot-customer-cursor',
      },
    });

    const stagedMutation = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateDraft($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Staged Draft After Snapshot',
            status: 'DRAFT',
          },
        },
      });

    expect(stagedMutation.status).toBe(200);
    expect(stagedMutation.body.data.productCreate.userErrors).toEqual([]);

    const resetResponse = await request(app).post('/__meta/reset');
    expect(resetResponse.status).toBe(200);

    const metaStateResponse = await request(app).get('/__meta/state');
    expect(metaStateResponse.status).toBe(200);
    expect(metaStateResponse.body.baseState.products['gid://shopify/Product/9001']).toMatchObject({
      id: 'gid://shopify/Product/9001',
      title: 'Snapshot Product',
    });
    expect(metaStateResponse.body.baseState.customers['gid://shopify/Customer/7001']).toMatchObject({
      id: 'gid://shopify/Customer/7001',
      displayName: 'Ada Snapshot',
    });
    expect(metaStateResponse.body.stagedState.products).toEqual({});

    const customersAfterReset = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'query SnapshotCustomers($query: String!) { customers(first: 1, query: $query, sortKey: UPDATED_AT, reverse: true) { edges { cursor node { id displayName } } pageInfo { startCursor endCursor } } }',
        variables: {
          query: 'state:ENABLED',
        },
      });

    expect(customersAfterReset.status).toBe(200);
    expect(customersAfterReset.body.data.customers).toEqual({
      edges: [
        {
          cursor: 'opaque-snapshot-customer-cursor',
          node: {
            id: 'gid://shopify/Customer/7001',
            displayName: 'Ada Snapshot',
          },
        },
      ],
      pageInfo: {
        startCursor: 'opaque-snapshot-customer-cursor',
        endCursor: 'opaque-snapshot-customer-cursor',
      },
    });
  });
});
