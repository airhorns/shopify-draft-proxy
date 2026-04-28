import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../support/runtime.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../support/runtime.js';
import { store } from '../support/runtime.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

describe('saved search flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('returns empty saved-search connections in snapshot mode without upstream access', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('snapshot reads must stay local'));
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query EmptySavedSearches($query: String!) {
          productSavedSearches(first: 2) { nodes { id } edges { cursor node { id } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          collectionSavedSearches(first: 2) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          customerSavedSearches(first: 2, query: $query) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          orderSavedSearches(first: 2) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          draftOrderSavedSearches(first: 2) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          fileSavedSearches(first: 2) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          codeDiscountSavedSearches(first: 2) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          automaticDiscountSavedSearches(first: 2) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          discountRedeemCodeSavedSearches(first: 2, query: $query) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
        }`,
        variables: { query: '__no_saved_search_match__' },
      });

    expect(response.status).toBe(200);
    const { draftOrderSavedSearches, orderSavedSearches, ...emptyConnections } = response.body.data;
    for (const value of Object.values(emptyConnections)) {
      expect(value).toMatchObject({
        nodes: [],
        pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
      });
    }
    expect(response.body.data.productSavedSearches.edges).toEqual([]);
    expect(orderSavedSearches.nodes).toEqual([
      { id: 'gid://shopify/SavedSearch/3634391515442' },
      { id: 'gid://shopify/SavedSearch/3634391548210' },
    ]);
    expect(orderSavedSearches.pageInfo).toMatchObject({
      hasNextPage: true,
      hasPreviousPage: false,
      startCursor: 'cursor:gid://shopify/SavedSearch/3634391515442',
      endCursor: 'cursor:gid://shopify/SavedSearch/3634391548210',
    });
    expect(draftOrderSavedSearches.nodes).toEqual([
      { id: 'gid://shopify/SavedSearch/3634390597938' },
      { id: 'gid://shopify/SavedSearch/3634390630706' },
    ]);
    expect(draftOrderSavedSearches.pageInfo).toMatchObject({
      hasNextPage: true,
      hasPreviousPage: false,
      startCursor: 'cursor:gid://shopify/SavedSearch/3634390597938',
      endCursor: 'cursor:gid://shopify/SavedSearch/3634390630706',
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages saved search create, update, and delete locally with downstream reads and log visibility', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('saved search mutations must stay local'));
    const app = createApp(config).callback();

    const createResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateSavedSearch($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) {
            savedSearch { id legacyResourceId name query resourceType searchTerms filters { key value } }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            resourceType: 'PRODUCT',
            name: 'Codex Products',
            query: 'title:Codex status:ACTIVE seasonal',
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.savedSearchCreate.userErrors).toEqual([]);
    expect(createResponse.body.data.savedSearchCreate.savedSearch).toMatchObject({
      legacyResourceId: '1',
      name: 'Codex Products',
      query: 'title:Codex status:ACTIVE seasonal',
      resourceType: 'PRODUCT',
      searchTerms: 'seasonal',
      filters: [
        { key: 'title', value: 'Codex' },
        { key: 'status', value: 'ACTIVE' },
      ],
    });
    const savedSearchId = createResponse.body.data.savedSearchCreate.savedSearch.id;

    const readAfterCreate = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadSavedSearches($query: String!) {
          productSavedSearches(first: 1, query: $query) {
            nodes { id name query resourceType searchTerms filters { key value } }
            edges { cursor node { id name } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          customerSavedSearches(first: 3, query: $query) { nodes { id name } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
        }`,
        variables: { query: 'Codex Products' },
      });

    expect(readAfterCreate.body.data.productSavedSearches).toMatchObject({
      nodes: [
        {
          id: savedSearchId,
          name: 'Codex Products',
          query: 'seasonal title:Codex status:ACTIVE',
          resourceType: 'PRODUCT',
          searchTerms: 'seasonal',
        },
      ],
      edges: [{ cursor: `cursor:${savedSearchId}`, node: { id: savedSearchId, name: 'Codex Products' } }],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: false,
        startCursor: `cursor:${savedSearchId}`,
        endCursor: `cursor:${savedSearchId}`,
      },
    });
    expect(readAfterCreate.body.data.customerSavedSearches.nodes).toEqual([]);

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateSavedSearch($input: SavedSearchUpdateInput!) {
          savedSearchUpdate(input: $input) {
            savedSearch { id name query resourceType searchTerms filters { key value } }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            id: savedSearchId,
            name: 'Codex Active',
            query: 'status:ACTIVE updated',
          },
        },
      });

    expect(updateResponse.body.data.savedSearchUpdate).toMatchObject({
      savedSearch: {
        id: savedSearchId,
        name: 'Codex Active',
        query: 'status:ACTIVE updated',
        resourceType: 'PRODUCT',
        searchTerms: 'updated',
        filters: [{ key: 'status', value: 'ACTIVE' }],
      },
      userErrors: [],
    });

    const partialValidationResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateSavedSearch($input: SavedSearchUpdateInput!) {
          savedSearchUpdate(input: $input) {
            savedSearch { id name query resourceType searchTerms filters { key value } }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            id: savedSearchId,
            name: 'Codex Active With A Name That Is Far Too Long For Shopify',
            query: 'status:DRAFT revalidated',
          },
        },
      });

    expect(partialValidationResponse.body.data.savedSearchUpdate).toMatchObject({
      savedSearch: {
        id: savedSearchId,
        name: 'Codex Active',
        query: 'status:DRAFT revalidated',
        resourceType: 'PRODUCT',
        searchTerms: 'revalidated',
        filters: [{ key: 'status', value: 'DRAFT' }],
      },
      userErrors: [{ field: ['input', 'name'], message: 'Name is too long (maximum is 40 characters)' }],
    });

    const readAfterPartialValidation = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadAfterPartialValidation($query: String!) {
          productSavedSearches(first: 1, query: $query) {
            nodes { id name query resourceType searchTerms filters { key value } }
          }
        }`,
        variables: { query: 'revalidated' },
      });

    expect(readAfterPartialValidation.body.data.productSavedSearches.nodes).toEqual([
      {
        id: savedSearchId,
        name: 'Codex Active',
        query: 'revalidated status:DRAFT',
        resourceType: 'PRODUCT',
        searchTerms: 'revalidated',
        filters: [{ key: 'status', value: 'DRAFT' }],
      },
    ]);

    const missingResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation MissingSavedSearch($input: SavedSearchUpdateInput!, $deleteInput: SavedSearchDeleteInput!) {
          savedSearchUpdate(input: $input) { savedSearch { id name } userErrors { field message } }
          savedSearchDelete(input: $deleteInput) { deletedSavedSearchId userErrors { field message } }
        }`,
        variables: {
          input: { id: 'gid://shopify/SavedSearch/0', name: 'Missing' },
          deleteInput: { id: 'gid://shopify/SavedSearch/0' },
        },
      });

    expect(missingResponse.body.data).toMatchObject({
      savedSearchUpdate: {
        savedSearch: null,
        userErrors: [{ field: ['input', 'id'], message: 'Saved Search does not exist' }],
      },
      savedSearchDelete: {
        deletedSavedSearchId: null,
        userErrors: [{ field: ['input', 'id'], message: 'Saved Search does not exist' }],
      },
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteSavedSearch($input: SavedSearchDeleteInput!) {
          savedSearchDelete(input: $input) { deletedSavedSearchId userErrors { field message } }
        }`,
        variables: { input: { id: savedSearchId } },
      });

    expect(deleteResponse.body.data.savedSearchDelete).toEqual({
      deletedSavedSearchId: savedSearchId,
      userErrors: [],
    });

    const readAfterDelete = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: `query ReadAfterDelete { productSavedSearches(first: 5) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }`,
    });
    expect(readAfterDelete.body.data.productSavedSearches).toMatchObject({
      nodes: [],
      pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
    });

    expect(store.getLog().map((entry) => entry.operationName)).toEqual([
      'savedSearchCreate',
      'savedSearchUpdate',
      'savedSearchUpdate',
      'savedSearchUpdate',
      'savedSearchDelete',
    ]);
    expect(store.getLog()[0]).toMatchObject({
      status: 'staged',
      interpreted: {
        capability: {
          domain: 'saved-searches',
          execution: 'stage-locally',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('routes staged saved searches through their resource-specific roots', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('saved search resource roots must stay local'));
    const app = createApp(config).callback();
    const created: Record<string, { id: string; name: string }> = {};

    for (const resourceType of [
      'PRODUCT',
      'COLLECTION',
      'ORDER',
      'DRAFT_ORDER',
      'FILE',
      'PRICE_RULE',
      'DISCOUNT_REDEEM_CODE',
    ]) {
      const name = `HAR-402 ${resourceType}`;
      const response = await request(app)
        .post('/admin/api/2026-04/graphql.json')
        .send({
          query: `mutation CreateSavedSearch($input: SavedSearchCreateInput!) {
            savedSearchCreate(input: $input) {
              savedSearch { id name resourceType }
              userErrors { field message }
            }
          }`,
          variables: {
            input: {
              resourceType,
              name,
              query: `title:${resourceType} har-402`,
            },
          },
        });

      expect(response.status).toBe(200);
      expect(response.body.data.savedSearchCreate.userErrors).toEqual([]);
      created[resourceType] = {
        id: response.body.data.savedSearchCreate.savedSearch.id,
        name,
      };
    }

    const readResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ResourceSpecificSavedSearches {
          products: productSavedSearches(first: 2, query: "HAR-402 PRODUCT") { nodes { id name resourceType } }
          collections: collectionSavedSearches(first: 2, query: "HAR-402 COLLECTION") { nodes { id name resourceType } }
          customers: customerSavedSearches(first: 2, query: "HAR-402 CUSTOMER") { nodes { id name resourceType } }
          orders: orderSavedSearches(first: 2, query: "HAR-402 ORDER") { nodes { id name resourceType } }
          draftOrders: draftOrderSavedSearches(first: 2, query: "HAR-402 DRAFT_ORDER") { nodes { id name resourceType } }
          files: fileSavedSearches(first: 2, query: "HAR-402 FILE") { nodes { id name resourceType } }
          codeDiscounts: codeDiscountSavedSearches(first: 2) { nodes { id name resourceType } }
          automaticDiscounts: automaticDiscountSavedSearches(first: 2) { nodes { id name resourceType } }
          redeemCodes: discountRedeemCodeSavedSearches(first: 2, query: "HAR-402 DISCOUNT_REDEEM_CODE") { nodes { id name resourceType } }
          misplacedProduct: collectionSavedSearches(first: 2, query: "HAR-402 PRODUCT") { nodes { id name resourceType } }
        }`,
      });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body.data).toMatchObject({
      products: { nodes: [{ id: created['PRODUCT']?.id, name: created['PRODUCT']?.name, resourceType: 'PRODUCT' }] },
      collections: {
        nodes: [{ id: created['COLLECTION']?.id, name: created['COLLECTION']?.name, resourceType: 'COLLECTION' }],
      },
      customers: { nodes: [] },
      orders: { nodes: [{ id: created['ORDER']?.id, name: created['ORDER']?.name, resourceType: 'ORDER' }] },
      draftOrders: {
        nodes: [{ id: created['DRAFT_ORDER']?.id, name: created['DRAFT_ORDER']?.name, resourceType: 'DRAFT_ORDER' }],
      },
      files: { nodes: [{ id: created['FILE']?.id, name: created['FILE']?.name, resourceType: 'FILE' }] },
      codeDiscounts: {
        nodes: [{ id: created['PRICE_RULE']?.id, name: created['PRICE_RULE']?.name, resourceType: 'PRICE_RULE' }],
      },
      automaticDiscounts: {
        nodes: [{ id: created['PRICE_RULE']?.id, name: created['PRICE_RULE']?.name, resourceType: 'PRICE_RULE' }],
      },
      redeemCodes: {
        nodes: [
          {
            id: created['DISCOUNT_REDEEM_CODE']?.id,
            name: created['DISCOUNT_REDEEM_CODE']?.name,
            resourceType: 'DISCOUNT_REDEEM_CODE',
          },
        ],
      },
      misplacedProduct: { nodes: [] },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('matches Shopify customer saved-search create deprecation', async () => {
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateCustomerSavedSearch($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) {
            savedSearch { id name resourceType }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            resourceType: 'CUSTOMER',
            name: 'HAR-402 Customer',
            query: 'HAR-402 CUSTOMER',
          },
        },
      });

    expect(response.body.data.savedSearchCreate).toEqual({
      savedSearch: null,
      userErrors: [
        {
          field: null,
          message: 'Customer saved searches have been deprecated. Use Segmentation API instead.',
        },
      ],
    });
  });

  it('does not claim URL redirect saved-search resource support without navigation conformance', async () => {
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateUrlRedirectSavedSearch($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) {
            savedSearch { id name resourceType }
            userErrors { field message }
          }
        }`,
        variables: {
          input: {
            resourceType: 'URL_REDIRECT',
            name: 'Redirects',
            query: '/old-path',
          },
        },
      });

    expect(response.body.data.savedSearchCreate).toEqual({
      savedSearch: null,
      userErrors: [
        {
          field: ['input', 'resourceType'],
          message: 'URL redirect saved searches require online-store navigation conformance before local support',
        },
      ],
    });
  });
});
