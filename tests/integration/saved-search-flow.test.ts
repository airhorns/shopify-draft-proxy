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
    const { draftOrderSavedSearches, ...emptyConnections } = response.body.data;
    for (const value of Object.values(emptyConnections)) {
      expect(value).toMatchObject({
        nodes: [],
        pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
      });
    }
    expect(response.body.data.productSavedSearches.edges).toEqual([]);
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
      query: 'seasonal title:Codex status:ACTIVE',
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
        query: 'updated status:ACTIVE',
        resourceType: 'PRODUCT',
        searchTerms: 'updated',
        filters: [{ key: 'status', value: 'ACTIVE' }],
      },
      userErrors: [],
    });

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
