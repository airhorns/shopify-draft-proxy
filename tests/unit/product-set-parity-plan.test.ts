import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('productSet parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged productSet create slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/productSet-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      blocker?: unknown;
      comparison?: {
        targets?: Array<{
          name?: string;
          capturePath?: string;
          proxyPath?: string;
          proxyRequest?: { documentPath?: string | null };
        }>;
        expectedDifferences?: Array<{ path?: string }>;
      };
      proxyRequest?: {
        documentPath?: string | null;
        variablesPath?: string | null;
        variablesCapturePath?: string | null;
      };
    };

    expect(spec.blocker).toBeUndefined();
    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/productSet-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/productSet-parity-plan.variables.json');
    expect(spec.proxyRequest?.variablesCapturePath).toBe('$.mutation.variables');
    expect(spec.comparison?.targets).toEqual([
      {
        name: 'mutation-data',
        capturePath: '$.mutation.response.data',
        proxyPath: '$.data',
      },
      {
        name: 'downstream-read-data',
        capturePath: '$.downstreamRead.data',
        proxyRequest: {
          documentPath: 'config/parity-requests/productSet-downstream-read.graphql',
          variables: {
            id: {
              fromPrimaryProxyPath: '$.data.productSet.product.id',
            },
          },
        },
        proxyPath: '$.data',
      },
    ]);
    expect(spec.comparison?.expectedDifferences?.map((difference) => difference.path)).toEqual([
      '$.productSet.product.id',
      '$.productSet.product.options[0].id',
      '$.productSet.product.options[0].optionValues[0].id',
      '$.productSet.product.options[0].optionValues[1].id',
      '$.productSet.product.variants["nodes"][0].id',
      '$.productSet.product.variants["nodes"][1].id',
      '$.productSet.product.metafields.nodes[0].id',
      '$.product.id',
      '$.product.onlineStorePreviewUrl',
      '$.product.options[0].id',
      '$.product.options[0].optionValues[0].id',
      '$.product.options[0].optionValues[1].id',
      '$.product.variants["nodes"][0].id',
      '$.product.variants["nodes"][0].inventoryItem.id',
      '$.product.variants["nodes"][1].id',
      '$.product.variants["nodes"][1].inventoryItem.id',
      '$.product.metafield.id',
      '$.product.metafields.nodes[0].id',
    ]);

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);
    const downstreamDocumentPath = resolve(repoRoot, spec.comparison!.targets![1]!.proxyRequest!.documentPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);
    expect(existsSync(downstreamDocumentPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const downstreamDocument = readFileSync(downstreamDocumentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      synchronous?: boolean;
      input?: Record<string, unknown> & {
        productOptions?: Array<Record<string, unknown>>;
        variants?: Array<Record<string, unknown>>;
        metafields?: Array<Record<string, unknown>>;
      };
    };

    expect(document).toContain('mutation ProductSetParityPlan($input: ProductSetInput!, $synchronous: Boolean!)');
    expect(document).toContain('productSet(input: $input, synchronous: $synchronous)');
    expect(document).toContain('productSetOperation {');
    expect(document).toContain('options {');
    expect(document).toContain('variants(first: 10) {');
    expect(document).toContain('metafields(first: 10) {');
    expect(downstreamDocument).toContain('query ProductSetDownstreamRead($id: ID!)');
    expect(downstreamDocument).toContain('product(id: $id)');
    expect(downstreamDocument).toContain('variants(first: 10) {');
    expect(downstreamDocument).toContain('metafield(namespace: "custom", key: "material") {');

    expect(variables.synchronous).toBe(true);
    expect(variables.input).toMatchObject({
      title: 'Parity Set Snowboard',
      status: 'DRAFT',
      vendor: 'BURTON',
      productType: 'SNOWBOARD',
      tags: ['parity-plan', 'winter'],
    });
    expect(variables.input?.productOptions).toEqual([
      {
        name: 'Color',
        position: 1,
        values: [{ name: 'Blue' }, { name: 'Black' }],
      },
    ]);
    expect(variables.input?.variants).toEqual([
      {
        optionValues: [{ optionName: 'Color', name: 'Blue' }],
        sku: 'PARITY-SET-BLUE',
        price: '79.99',
        inventoryQuantities: [{ quantity: 7 }],
        inventoryItem: { tracked: true, requiresShipping: true },
      },
      {
        optionValues: [{ optionName: 'Color', name: 'Black' }],
        sku: 'PARITY-SET-BLACK',
        price: '69.99',
        inventoryQuantities: [{ quantity: 3 }],
        inventoryItem: { tracked: false, requiresShipping: true },
      },
    ]);
    expect(variables.input?.metafields).toEqual([
      {
        namespace: 'custom',
        key: 'season',
        type: 'single_line_text_field',
        value: 'winter',
      },
    ]);
  });
});
