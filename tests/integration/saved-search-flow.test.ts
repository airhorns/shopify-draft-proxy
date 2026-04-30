import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { beforeAll, describe, expect, it } from 'vitest';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');
const gleamProjectRoot = resolve(repoRoot, 'gleam');
const compiledEntrypoint = resolve(
  gleamProjectRoot,
  'build/dev/javascript/shopify_draft_proxy/shopify_draft_proxy.mjs',
);

type DraftProxy = {
  processRequest(request: {
    method: string;
    path: string;
    body?: unknown;
  }): Promise<{ status: number; body: Record<string, unknown> }>;
  getLog(): { entries: Array<Record<string, unknown>> };
};

type GleamShim = {
  createDraftProxy(config: { readMode: 'snapshot'; port: number; shopifyAdminOrigin: string }): DraftProxy;
};

async function loadShim(): Promise<GleamShim> {
  return (await import(resolve(gleamProjectRoot, 'js/src/index.ts'))) as GleamShim;
}

async function createProxy(): Promise<DraftProxy> {
  const shim = await loadShim();
  return shim.createDraftProxy({
    readMode: 'snapshot',
    port: 4000,
    shopifyAdminOrigin: 'https://example.myshopify.com',
  });
}

async function graphql(
  proxy: DraftProxy,
  body: { query: string; variables?: Record<string, unknown> },
): Promise<{ status: number; body: Record<string, unknown> }> {
  return proxy.processRequest({
    method: 'POST',
    path: '/admin/api/2026-04/graphql.json',
    body,
  });
}

describe('saved search flow through the Gleam shim', () => {
  beforeAll(() => {
    if (!existsSync(compiledEntrypoint)) {
      execFileSync('gleam', ['build', '--target', 'javascript'], {
        cwd: gleamProjectRoot,
        stdio: 'inherit',
      });
    }
  });

  it('returns empty saved-search connections in snapshot mode without upstream access', async () => {
    const proxy = await createProxy();
    const response = await graphql(proxy, {
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
    const data = response.body['data'] as Record<string, unknown>;
    const { draftOrderSavedSearches, orderSavedSearches, ...emptyConnections } = data;
    for (const value of Object.values(emptyConnections)) {
      expect(value).toMatchObject({
        nodes: [],
        pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
      });
    }
    expect(data['productSavedSearches']).toMatchObject({ edges: [] });
    expect(orderSavedSearches).toMatchObject({
      nodes: [{ id: 'gid://shopify/SavedSearch/3634391515442' }, { id: 'gid://shopify/SavedSearch/3634391548210' }],
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/SavedSearch/3634391515442',
        endCursor: 'cursor:gid://shopify/SavedSearch/3634391548210',
      },
    });
    expect(draftOrderSavedSearches).toMatchObject({
      nodes: [{ id: 'gid://shopify/SavedSearch/3634390597938' }, { id: 'gid://shopify/SavedSearch/3634390630706' }],
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/SavedSearch/3634390597938',
        endCursor: 'cursor:gid://shopify/SavedSearch/3634390630706',
      },
    });
  });

  it('stages saved search create, update, and delete locally with downstream reads and log visibility', async () => {
    const proxy = await createProxy();
    const createResponse = await graphql(proxy, {
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

    const createPayload = ((createResponse.body['data'] as Record<string, unknown>)['savedSearchCreate'] ??
      {}) as Record<string, unknown>;
    expect(createPayload).toMatchObject({
      savedSearch: {
        legacyResourceId: '1',
        name: 'Codex Products',
        query: 'title:Codex status:ACTIVE seasonal',
        resourceType: 'PRODUCT',
        searchTerms: 'seasonal',
        filters: [
          { key: 'title', value: 'Codex' },
          { key: 'status', value: 'ACTIVE' },
        ],
      },
      userErrors: [],
    });
    const savedSearchId = (createPayload['savedSearch'] as Record<string, unknown>)['id'];

    const readAfterCreate = await graphql(proxy, {
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
    const readAfterCreateData = readAfterCreate.body['data'] as Record<string, unknown>;

    expect(readAfterCreateData['productSavedSearches']).toMatchObject({
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
    expect(readAfterCreateData['customerSavedSearches']).toMatchObject({ nodes: [] });

    const updateResponse = await graphql(proxy, {
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

    expect((updateResponse.body['data'] as Record<string, unknown>)['savedSearchUpdate']).toMatchObject({
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

    const partialValidationResponse = await graphql(proxy, {
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

    expect((partialValidationResponse.body['data'] as Record<string, unknown>)['savedSearchUpdate']).toMatchObject({
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

    const missingResponse = await graphql(proxy, {
      query: `mutation MissingSavedSearch($input: SavedSearchUpdateInput!, $deleteInput: SavedSearchDeleteInput!) {
        savedSearchUpdate(input: $input) { savedSearch { id name } userErrors { field message } }
        savedSearchDelete(input: $deleteInput) { deletedSavedSearchId userErrors { field message } }
      }`,
      variables: {
        input: { id: 'gid://shopify/SavedSearch/0', name: 'Missing' },
        deleteInput: { id: 'gid://shopify/SavedSearch/0' },
      },
    });

    expect(missingResponse.body['data']).toMatchObject({
      savedSearchUpdate: {
        savedSearch: null,
        userErrors: [{ field: ['input', 'id'], message: 'Saved Search does not exist' }],
      },
      savedSearchDelete: {
        deletedSavedSearchId: null,
        userErrors: [{ field: ['input', 'id'], message: 'Saved Search does not exist' }],
      },
    });

    const deleteResponse = await graphql(proxy, {
      query: `mutation DeleteSavedSearch($input: SavedSearchDeleteInput!) {
        savedSearchDelete(input: $input) { deletedSavedSearchId userErrors { field message } }
      }`,
      variables: { input: { id: savedSearchId } },
    });

    expect((deleteResponse.body['data'] as Record<string, unknown>)['savedSearchDelete']).toEqual({
      deletedSavedSearchId: savedSearchId,
      userErrors: [],
    });

    const readAfterDelete = await graphql(proxy, {
      query: `query ReadAfterDelete {
        productSavedSearches(first: 5) { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
      }`,
    });
    expect((readAfterDelete.body['data'] as Record<string, unknown>)['productSavedSearches']).toMatchObject({
      nodes: [],
      pageInfo: { hasNextPage: false, hasPreviousPage: false, startCursor: null, endCursor: null },
    });

    expect(proxy.getLog().entries.map((entry) => entry['operationName'])).toEqual([
      'savedSearchCreate',
      'savedSearchUpdate',
      'savedSearchUpdate',
      'savedSearchUpdate',
      'savedSearchDelete',
      'savedSearchDelete',
    ]);
    expect(proxy.getLog().entries[0]).toMatchObject({
      status: 'staged',
      interpreted: {
        capability: {
          domain: 'saved-searches',
          execution: 'stage-locally',
        },
      },
    });
  });

  it('normalizes captured Shopify saved-search query grammar for downstream reads', async () => {
    const proxy = await createProxy();
    const createResponse = await graphql(proxy, {
      query: `mutation CreateSavedSearch($input: SavedSearchCreateInput!) {
        savedSearchCreate(input: $input) {
          savedSearch { id legacyResourceId name query resourceType searchTerms filters { key value } }
          userErrors { field message }
        }
      }`,
      variables: {
        input: {
          resourceType: 'PRODUCT',
          name: 'HAR-458 Grammar',
          query: "title:'HAR-458 Alpha' OR (status:ACTIVE tag:'HAR-458-tag') -vendor:Archived",
        },
      },
    });

    const createPayload = (createResponse.body['data'] as Record<string, Record<string, unknown>>)[
      'savedSearchCreate'
    ]!;
    expect(createPayload).toMatchObject({
      savedSearch: {
        name: 'HAR-458 Grammar',
        query: "title:'HAR-458 Alpha' OR (status:ACTIVE tag:'HAR-458-tag') -vendor:Archived",
        resourceType: 'PRODUCT',
        searchTerms: 'title:"HAR-458 Alpha" OR (status:ACTIVE tag:"HAR-458-tag")',
        filters: [{ key: 'vendor_not', value: 'Archived' }],
      },
      userErrors: [],
    });
    const savedSearchId = (createPayload['savedSearch'] as Record<string, unknown>)['id'];

    const readResponse = await graphql(proxy, {
      query: `query ReadSavedSearches($query: String!) {
        productSavedSearches(first: 1, query: $query) {
          nodes { id name query resourceType searchTerms filters { key value } }
        }
      }`,
      variables: { query: 'HAR-458 Grammar' },
    });

    expect((readResponse.body['data'] as Record<string, unknown>)['productSavedSearches']).toMatchObject({
      nodes: [
        {
          id: savedSearchId,
          name: 'HAR-458 Grammar',
          query: 'title:"HAR-458 Alpha" OR (status:ACTIVE tag:"HAR-458-tag") -vendor:Archived',
          resourceType: 'PRODUCT',
          searchTerms: 'title:"HAR-458 Alpha" OR (status:ACTIVE tag:"HAR-458-tag")',
          filters: [{ key: 'vendor_not', value: 'Archived' }],
        },
      ],
    });
  });

  it('routes staged saved searches through their resource-specific roots', async () => {
    const proxy = await createProxy();
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
      const response = await graphql(proxy, {
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
      const payload = ((response.body['data'] as Record<string, unknown>)['savedSearchCreate'] ?? {}) as Record<
        string,
        unknown
      >;
      expect(payload['userErrors']).toEqual([]);
      created[resourceType] = {
        id: ((payload['savedSearch'] as Record<string, unknown>)['id'] ?? '') as string,
        name,
      };
    }

    const readResponse = await graphql(proxy, {
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
    expect(readResponse.body['data']).toMatchObject({
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
  });

  it('matches Shopify customer saved-search create deprecation', async () => {
    const proxy = await createProxy();
    const response = await graphql(proxy, {
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

    expect((response.body['data'] as Record<string, unknown>)['savedSearchCreate']).toEqual({
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
    const proxy = await createProxy();
    const response = await graphql(proxy, {
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

    expect((response.body['data'] as Record<string, unknown>)['savedSearchCreate']).toEqual({
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
