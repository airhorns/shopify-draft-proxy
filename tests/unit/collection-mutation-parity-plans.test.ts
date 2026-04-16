import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

type ProxyRequestSpec = {
  proxyRequest?: {
    documentPath?: string | null;
    variablesPath?: string | null;
  };
};

function expectParityPlanScaffold(options: {
  specPath: string;
  documentPath: string;
  variablesPath: string;
  expectedDocumentSnippets: string[];
  expectedVariables: Record<string, unknown>;
}) {
  const repoRoot = resolve(import.meta.dirname, '../..');
  const spec = JSON.parse(readFileSync(resolve(repoRoot, options.specPath), 'utf8')) as ProxyRequestSpec;

  expect(spec.proxyRequest?.documentPath).toBe(options.documentPath);
  expect(spec.proxyRequest?.variablesPath).toBe(options.variablesPath);

  const documentAbsolutePath = resolve(repoRoot, options.documentPath);
  const variablesAbsolutePath = resolve(repoRoot, options.variablesPath);

  expect(existsSync(documentAbsolutePath)).toBe(true);
  expect(existsSync(variablesAbsolutePath)).toBe(true);

  const document = readFileSync(documentAbsolutePath, 'utf8');
  const variables = JSON.parse(readFileSync(variablesAbsolutePath, 'utf8')) as Record<string, unknown>;

  for (const snippet of options.expectedDocumentSnippets) {
    expect(document).toContain(snippet);
  }

  expect(variables).toMatchObject(options.expectedVariables);
}

describe('collection mutation parity plan scaffolds', () => {
  it('declares a concrete proxy request scaffold for collectionCreate', () => {
    expectParityPlanScaffold({
      specPath: 'config/parity-specs/collectionCreate-parity-plan.json',
      documentPath: 'config/parity-requests/collectionCreate-parity-plan.graphql',
      variablesPath: 'config/parity-requests/collectionCreate-parity-plan.variables.json',
      expectedDocumentSnippets: [
        'mutation CollectionCreateParityPlan($input: CollectionInput!)',
        'collectionCreate(input: $input)',
        'collection {',
        'products(first: 10)',
        'pageInfo {',
        'userErrors {',
        'field',
        'message',
      ],
      expectedVariables: {
        input: {
          title: 'Parity Plan Winter Hats',
        },
      },
    });
  });

  it('declares a concrete proxy request scaffold for collectionUpdate', () => {
    expectParityPlanScaffold({
      specPath: 'config/parity-specs/collectionUpdate-parity-plan.json',
      documentPath: 'config/parity-requests/collectionUpdate-parity-plan.graphql',
      variablesPath: 'config/parity-requests/collectionUpdate-parity-plan.variables.json',
      expectedDocumentSnippets: [
        'mutation CollectionUpdateParityPlan($input: CollectionInput!)',
        'collectionUpdate(input: $input)',
        'handle',
        'products(first: 10)',
        'nodes {',
        'userErrors {',
        'field',
        'message',
      ],
      expectedVariables: {
        input: {
          id: 'gid://shopify/Collection/900',
          title: 'Hydrated Collection Draft',
          handle: 'hydrated-collection-draft',
        },
      },
    });
  });

  it('declares a concrete proxy request scaffold for collectionDelete', () => {
    expectParityPlanScaffold({
      specPath: 'config/parity-specs/collectionDelete-parity-plan.json',
      documentPath: 'config/parity-requests/collectionDelete-parity-plan.graphql',
      variablesPath: 'config/parity-requests/collectionDelete-parity-plan.variables.json',
      expectedDocumentSnippets: [
        'mutation CollectionDeleteParityPlan($input: CollectionDeleteInput!)',
        'collectionDelete(input: $input)',
        'deletedCollectionId',
        'userErrors {',
        'field',
        'message',
      ],
      expectedVariables: {
        input: {
          id: 'gid://shopify/Collection/901',
        },
      },
    });
  });

  it('declares a concrete proxy request scaffold for collectionAddProducts', () => {
    expectParityPlanScaffold({
      specPath: 'config/parity-specs/collectionAddProducts-parity-plan.json',
      documentPath: 'config/parity-requests/collectionAddProducts-parity-plan.graphql',
      variablesPath: 'config/parity-requests/collectionAddProducts-parity-plan.variables.json',
      expectedDocumentSnippets: [
        'mutation CollectionAddProductsParityPlan($id: ID!, $productIds: [ID!]!)',
        'collectionAddProducts(id: $id, productIds: $productIds)',
        'products(first: 10)',
        'title',
        'handle',
        'userErrors {',
        'field',
        'message',
      ],
      expectedVariables: {
        id: 'gid://shopify/Collection/930',
        productIds: ['gid://shopify/Product/30', 'gid://shopify/Product/31'],
      },
    });
  });

  it('declares a concrete proxy request scaffold for collectionRemoveProducts', () => {
    expectParityPlanScaffold({
      specPath: 'config/parity-specs/collectionRemoveProducts-parity-plan.json',
      documentPath: 'config/parity-requests/collectionRemoveProducts-parity-plan.graphql',
      variablesPath: 'config/parity-requests/collectionRemoveProducts-parity-plan.variables.json',
      expectedDocumentSnippets: [
        'mutation CollectionRemoveProductsParityPlan($id: ID!, $productIds: [ID!]!)',
        'collectionRemoveProducts(id: $id, productIds: $productIds)',
        'job {',
        'done',
        'userErrors {',
        'field',
        'message',
      ],
      expectedVariables: {
        id: 'gid://shopify/Collection/950',
        productIds: ['gid://shopify/Product/50', 'gid://shopify/Product/999999'],
      },
    });
  });
});
