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

describe('product variant mutation parity plan scaffolds', () => {
  it('declares a concrete proxy request scaffold for productVariantsBulkUpdate', () => {
    expectParityPlanScaffold({
      specPath: 'config/parity-specs/productVariantsBulkUpdate-parity-plan.json',
      documentPath: 'config/parity-requests/productVariantsBulkUpdate-parity-plan.graphql',
      variablesPath: 'config/parity-requests/productVariantsBulkUpdate-parity-plan.variables.json',
      expectedDocumentSnippets: [
        'mutation ProductVariantsBulkUpdateParityPlan($productId: ID!, $variants: [ProductVariantsBulkInput!]!)',
        'productVariantsBulkUpdate(productId: $productId, variants: $variants)',
        'product {',
        'totalInventory',
        'tracksInventory',
        'variants(first: 10) {',
        'productVariants {',
        'compareAtPrice',
        'taxable',
        'inventoryPolicy',
        'inventoryItem {',
        'requiresShipping',
        'userErrors {',
      ],
      expectedVariables: {
        productId: 'gid://shopify/Product/100',
        variants: [
          {
            id: 'gid://shopify/ProductVariant/200',
            barcode: '1111111111111',
            price: '24.00',
            compareAtPrice: '30.00',
            taxable: true,
            inventoryPolicy: 'DENY',
            inventoryItem: {
              sku: 'HAT-DEFAULT-BLACK',
              tracked: true,
              requiresShipping: true,
            },
          },
        ],
      },
    });
  });

  it('declares a concrete proxy request scaffold for productVariantsBulkDelete', () => {
    expectParityPlanScaffold({
      specPath: 'config/parity-specs/productVariantsBulkDelete-parity-plan.json',
      documentPath: 'config/parity-requests/productVariantsBulkDelete-parity-plan.graphql',
      variablesPath: 'config/parity-requests/productVariantsBulkDelete-parity-plan.variables.json',
      expectedDocumentSnippets: [
        'mutation ProductVariantsBulkDeleteParityPlan($productId: ID!, $variantsIds: [ID!]!)',
        'productVariantsBulkDelete(productId: $productId, variantsIds: $variantsIds)',
        'product {',
        'totalInventory',
        'tracksInventory',
        'variants(first: 10) {',
        'nodes {',
        'title',
        'sku',
        'inventoryQuantity',
        'userErrors {',
      ],
      expectedVariables: {
        productId: 'gid://shopify/Product/100',
        variantsIds: ['gid://shopify/ProductVariant/200'],
      },
    });
  });

  it('declares a concrete proxy request scaffold for productVariantCreate', () => {
    expectParityPlanScaffold({
      specPath: 'config/parity-specs/productVariantCreate-parity-plan.json',
      documentPath: 'config/parity-requests/productVariantCreate-parity-plan.graphql',
      variablesPath: 'config/parity-requests/productVariantCreate-parity-plan.variables.json',
      expectedDocumentSnippets: [
        'mutation ProductVariantCreateParityPlan($input: ProductVariantInput!)',
        'productVariantCreate(input: $input)',
        'product {',
        'totalInventory',
        'tracksInventory',
        'productVariant {',
        'selectedOptions {',
        'inventoryItem {',
        'requiresShipping',
        'userErrors {',
      ],
      expectedVariables: {
        input: {
          productId: 'gid://shopify/Product/100',
          title: 'Blue / Large',
          sku: 'SVH-BL-L',
          barcode: '3333333333333',
          price: '29.00',
          inventoryQuantity: 8,
          selectedOptions: [
            { name: 'Color', value: 'Blue' },
            { name: 'Size', value: 'Large' },
          ],
          inventoryItem: {
            tracked: true,
            requiresShipping: false,
          },
        },
      },
    });
  });

  it('declares a concrete proxy request scaffold for productVariantUpdate', () => {
    expectParityPlanScaffold({
      specPath: 'config/parity-specs/productVariantUpdate-parity-plan.json',
      documentPath: 'config/parity-requests/productVariantUpdate-parity-plan.graphql',
      variablesPath: 'config/parity-requests/productVariantUpdate-parity-plan.variables.json',
      expectedDocumentSnippets: [
        'mutation ProductVariantUpdateParityPlan($input: ProductVariantInput!)',
        'productVariantUpdate(input: $input)',
        'product {',
        'totalInventory',
        'tracksInventory',
        'productVariant {',
        'selectedOptions {',
        'inventoryItem {',
        'requiresShipping',
        'userErrors {',
      ],
      expectedVariables: {
        input: {
          id: 'gid://shopify/ProductVariant/210',
          title: 'Blue / XL',
          sku: 'SVH-BL-XL',
          inventoryQuantity: 5,
          selectedOptions: [
            { name: 'Color', value: 'Blue' },
            { name: 'Size', value: 'XL' },
          ],
          inventoryItem: {
            tracked: true,
            requiresShipping: true,
          },
        },
      },
    });
  });

  it('declares a concrete proxy request scaffold for productVariantDelete', () => {
    expectParityPlanScaffold({
      specPath: 'config/parity-specs/productVariantDelete-parity-plan.json',
      documentPath: 'config/parity-requests/productVariantDelete-parity-plan.graphql',
      variablesPath: 'config/parity-requests/productVariantDelete-parity-plan.variables.json',
      expectedDocumentSnippets: [
        'mutation ProductVariantDeleteParityPlan($id: ID!)',
        'productVariantDelete(id: $id)',
        'deletedProductVariantId',
        'userErrors {',
        'field',
        'message',
      ],
      expectedVariables: {
        id: 'gid://shopify/ProductVariant/210',
      },
    });
  });
});
