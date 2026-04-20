import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

type ExpectedReadSpec = {
  specPath: string;
  documentPath: string;
  variablesPath: string;
  documentChecks: string[];
  variableChecks: Record<string, unknown>;
};

const expectedReadSpecs: ExpectedReadSpec[] = [
  {
    specPath: 'config/parity-specs/product-detail-read.json',
    documentPath: 'config/parity-requests/product-detail-read.graphql',
    variablesPath: 'config/parity-requests/product-detail-read.variables.json',
    documentChecks: [
      'query ProductDetailRead($id: ID!)',
      'product(id: $id)',
      'descriptionHtml',
      'collections(first: 3)',
      'media(first: 5)',
    ],
    variableChecks: {
      id: 'gid://shopify/Product/8397256720617',
    },
  },
  {
    specPath: 'config/parity-specs/products-catalog-read.json',
    documentPath: 'config/parity-requests/products-catalog-read.graphql',
    variablesPath: 'config/parity-requests/products-catalog-read.variables.json',
    documentChecks: [
      'query ProductsCatalogRead($first: Int!)',
      'productsCount',
      'products(first: $first, sortKey: UPDATED_AT, reverse: true)',
      'legacyResourceId',
      'pageInfo {',
    ],
    variableChecks: {
      first: 3,
    },
  },
  {
    specPath: 'config/parity-specs/products-search-read.json',
    documentPath: 'config/parity-requests/products-search-read.graphql',
    variablesPath: 'config/parity-requests/products-search-read.variables.json',
    documentChecks: [
      'query ProductsSearchRead($nikeQuery: String!, $inventoryQuery: String!)',
      'total: productsCount',
      'nike: products(first: 2, query: $nikeQuery)',
      'lowInventory: products(first: 2, query: $inventoryQuery)',
      'totalInventory',
    ],
    variableChecks: {
      nikeQuery: 'vendor:NIKE',
      inventoryQuery: 'inventory_total:<=0',
    },
  },
  {
    specPath: 'config/parity-specs/products-search-grammar-read.json',
    documentPath: 'config/parity-requests/products-search-grammar-read.graphql',
    variablesPath: 'config/parity-requests/products-search-grammar-read.variables.json',
    documentChecks: [
      'query ProductsSearchGrammarRead($phraseQuery: String!)',
      'phraseCount: productsCount(query: $phraseQuery)',
      'phraseMatches: products(first: 5, query: $phraseQuery)',
      'vendor',
      'productType',
      'tags',
    ],
    variableChecks: {
      phraseQuery: '"flat peak cap" accessories -vendor:VANS -tag:vans',
    },
  },
  {
    specPath: 'config/parity-specs/products-variant-search-read.json',
    documentPath: 'config/parity-requests/products-variant-search-read.graphql',
    variablesPath: 'config/parity-requests/products-variant-search-read.variables.json',
    documentChecks: [
      'query ProductsVariantSearchRead($query: String!)',
      'matches: productsCount(query: $query)',
      'products(first: 5, query: $query)',
      'handle',
      'pageInfo {',
    ],
    variableChecks: {
      query: 'sku:C-03-black-5',
    },
  },
  {
    specPath: 'config/parity-specs/products-advanced-search-read.json',
    documentPath: 'config/parity-requests/products-advanced-search-read.graphql',
    variablesPath: 'config/parity-requests/products-advanced-search-read.variables.json',
    documentChecks: [
      'query ProductsAdvancedSearchRead($prefixQuery: String!, $orQuery: String!, $groupedExclusionQuery: String!)',
      'prefixCount: productsCount(query: $prefixQuery)',
      'prefix: products(first: 3, query: $prefixQuery, sortKey: UPDATED_AT, reverse: true)',
      'orCount: productsCount(query: $orQuery)',
      'orMatches: products(first: 5, query: $orQuery, sortKey: UPDATED_AT, reverse: true)',
      'groupedExclusionCount: productsCount(query: $groupedExclusionQuery)',
      'groupedExclusion: products(first: 5, query: $groupedExclusionQuery, sortKey: UPDATED_AT, reverse: true)',
      'pageInfo {',
      'updatedAt',
    ],
    variableChecks: {
      prefixQuery: 'swoo* status:active',
      orQuery: '(vendor:NIKE OR vendor:VANS) tag:egnition-sample-data product_type:ACCESSORIES',
      groupedExclusionQuery: 'tag:egnition-sample-data product_type:ACCESSORIES -(vendor:VANS)',
    },
  },
  {
    specPath: 'config/parity-specs/products-or-precedence-read.json',
    documentPath: 'config/parity-requests/products-or-precedence-read.graphql',
    variablesPath: 'config/parity-requests/products-or-precedence-read.variables.json',
    documentChecks: [
      'query ProductsOrPrecedenceRead($precedenceQuery: String!)',
      'precedenceCount: productsCount(query: $precedenceQuery)',
      'precedenceMatches: products(first: 10, query: $precedenceQuery, sortKey: UPDATED_AT, reverse: true)',
      'pageInfo {',
      'updatedAt',
      'productType',
      'tags',
    ],
    variableChecks: {
      precedenceQuery: 'vendor:NIKE OR vendor:VANS tag:egnition-sample-data product_type:ACCESSORIES',
    },
  },
  {
    specPath: 'config/parity-specs/products-sort-keys-read.json',
    documentPath: 'config/parity-requests/products-sort-keys-read.graphql',
    variablesPath: 'config/parity-requests/products-sort-keys-read.variables.json',
    documentChecks: [
      'query ProductsSortKeysRead($first: Int!, $query: String!)',
      'titleOrder: products(first: $first, query: $query, sortKey: TITLE)',
      'vendorOrder: products(first: $first, query: $query, sortKey: VENDOR)',
      'productTypeOrder: products(first: $first, query: $query, sortKey: PRODUCT_TYPE, reverse: true)',
      'cursor',
      'pageInfo {',
      'handle',
      'status',
      'vendor',
      'productType',
    ],
    variableChecks: {
      first: 5,
      query: 'egnition-sample-data status:active',
    },
  },
  {
    specPath: 'config/parity-specs/products-relevance-search-read.json',
    documentPath: 'config/parity-requests/products-relevance-search-read.graphql',
    variablesPath: 'config/parity-requests/products-relevance-search-read.variables.json',
    documentChecks: [
      'query ProductsRelevanceSearchRead($first: Int!, $query: String!)',
      'products(first: $first, query: $query, sortKey: RELEVANCE)',
      'cursor',
      'legacyResourceId',
      'pageInfo {',
    ],
    variableChecks: {
      first: 3,
      query: 'swoo* status:active',
    },
  },
  {
    specPath: 'config/parity-specs/products-search-pagination-read.json',
    documentPath: 'config/parity-requests/products-search-pagination-read.graphql',
    variablesPath: 'config/parity-requests/products-search-pagination-read.variables.json',
    documentChecks: [
      'query ProductsSearchPaginationRead($query: String!, $afterCursor: String!, $beforeCursor: String!)',
      'count: productsCount(query: $query)',
      'firstPage: products(first: 1, query: $query, sortKey: UPDATED_AT, reverse: true)',
      'nextPage: products(first: 1, after: $afterCursor, query: $query, sortKey: UPDATED_AT, reverse: true)',
      'previousPage: products(last: 1, before: $beforeCursor, query: $query, sortKey: UPDATED_AT, reverse: true)',
      'cursor',
      'updatedAt',
      'pageInfo {',
    ],
    variableChecks: {
      query: 'tag:egnition-sample-data product_type:ACCESSORIES',
    },
  },
  {
    specPath: 'config/parity-specs/product-variants-read.json',
    documentPath: 'config/parity-requests/product-variants-read.graphql',
    variablesPath: 'config/parity-requests/product-variants-read.variables.json',
    documentChecks: [
      'query ProductVariantsRead($variantId: ID!, $inventoryItemId: ID!)',
      'variant: productVariant(id: $variantId)',
      'stock: inventoryItem(id: $inventoryItemId)',
      'selectedOptions {',
      'measurement {',
      'inventoryLevels(first: 5)',
      'quantities(names: ["available", "on_hand", "incoming"])',
      'location {',
      'product {',
    ],
    variableChecks: {
      variantId: 'gid://shopify/ProductVariant/46789263425769',
      inventoryItemId: 'gid://shopify/InventoryItem/48886350676201',
    },
  },
  {
    specPath: 'config/parity-specs/product-metafields-read.json',
    documentPath: 'config/parity-requests/product-metafields-read.graphql',
    variablesPath: 'config/parity-requests/product-metafields-read.variables.json',
    documentChecks: [
      'query ProductMetafieldsRead($id: ID!, $namespace: String!, $key: String!)',
      'product(id: $id)',
      'metafield(namespace: $namespace, key: $key)',
      'metafields(first: 5)',
      'pageInfo {',
    ],
    variableChecks: {
      id: 'gid://shopify/Product/8397256720617',
      namespace: 'custom',
      key: 'material',
    },
  },
];

describe('product read parity specs', () => {
  it('declares concrete proxy request scaffolds for the captured product read family', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');

    for (const expected of expectedReadSpecs) {
      const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.specPath), 'utf8')) as {
        proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
      };

      expect(spec.proxyRequest?.documentPath).toBe(expected.documentPath);
      expect(spec.proxyRequest?.variablesPath).toBe(expected.variablesPath);

      const documentPath = resolve(repoRoot, expected.documentPath);
      const variablesPath = resolve(repoRoot, expected.variablesPath);

      expect(existsSync(documentPath)).toBe(true);
      expect(existsSync(variablesPath)).toBe(true);

      const document = readFileSync(documentPath, 'utf8');
      const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as Record<string, unknown>;

      for (const check of expected.documentChecks) {
        expect(document).toContain(check);
      }

      expect(variables).toMatchObject(expected.variableChecks);
    }
  });
});
