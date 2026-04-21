import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import type { ParitySpec } from '../../scripts/conformance-parity-lib.js';

const expectedPlans = [
  {
    scenarioId: 'product-create-media-live-parity',
    specPath: 'config/parity-specs/productCreateMedia-parity-plan.json',
    documentPath: 'config/parity-requests/productCreateMedia-parity-plan.graphql',
    variablesPath: 'config/parity-requests/productCreateMedia-parity-plan.variables.json',
    operationLine: 'mutation ProductCreateMediaParityPlan($productId: ID!, $media: [CreateMediaInput!]!)',
    rootCall: 'productCreateMedia(productId: $productId, media: $media)',
  },
  {
    scenarioId: 'product-update-media-live-parity',
    specPath: 'config/parity-specs/productUpdateMedia-parity-plan.json',
    documentPath: 'config/parity-requests/productUpdateMedia-parity-plan.graphql',
    variablesPath: 'config/parity-requests/productUpdateMedia-parity-plan.variables.json',
    operationLine: 'mutation ProductUpdateMediaParityPlan($productId: ID!, $media: [UpdateMediaInput!]!)',
    rootCall: 'productUpdateMedia(productId: $productId, media: $media)',
  },
  {
    scenarioId: 'product-delete-media-live-parity',
    specPath: 'config/parity-specs/productDeleteMedia-parity-plan.json',
    documentPath: 'config/parity-requests/productDeleteMedia-parity-plan.graphql',
    variablesPath: 'config/parity-requests/productDeleteMedia-parity-plan.variables.json',
    operationLine: 'mutation ProductDeleteMediaParityPlan($productId: ID!, $mediaIds: [ID!]!)',
    rootCall: 'productDeleteMedia(productId: $productId, mediaIds: $mediaIds)',
  },
] as const;

describe('product media parity plan scaffolds', () => {
  it('declares concrete proxy request scaffolds for the staged media mutation family', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');

    for (const expected of expectedPlans) {
      const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.specPath), 'utf8')) as ParitySpec;
      expect(spec.scenarioId).toBe(expected.scenarioId);
      expect(spec.proxyRequest).toEqual(
        expect.objectContaining({
          documentPath: expected.documentPath,
          variablesPath: expected.variablesPath,
        }),
      );

      const documentPath = resolve(repoRoot, expected.documentPath);
      const variablesPath = resolve(repoRoot, expected.variablesPath);
      expect(existsSync(documentPath)).toBe(true);
      expect(existsSync(variablesPath)).toBe(true);

      const document = readFileSync(documentPath, 'utf8');
      expect(document).toContain(expected.operationLine);
      expect(document).toContain(expected.rootCall);
      expect(document).toContain('mediaUserErrors {');
      if (expected.scenarioId === 'product-delete-media-live-parity') {
        expect(document).toContain('deletedMediaIds');
        expect(document).toContain('deletedProductImageIds');
      } else {
        expect(document).toContain('media {');
        expect(document).toContain('status');
        expect(document).toContain('preview {');
        expect(document).toContain('... on MediaImage {');
      }
    }
  });

  it('promotes productCreateMedia to explicit mutation and downstream read comparisons', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const spec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/productCreateMedia-parity-plan.json'), 'utf8'),
    ) as ParitySpec;

    expect(spec.blocker).toBeUndefined();
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
          documentPath: 'config/parity-requests/productCreateMedia-downstream-read.graphql',
          variables: {
            id: {
              fromPrimaryProxyPath: '$.data.productCreateMedia.product.id',
            },
          },
        },
        proxyPath: '$.data',
      },
    ]);
    expect(spec.comparison?.expectedDifferences).toEqual([
      expect.objectContaining({
        path: '$.productCreateMedia.media[0].id',
        matcher: 'shopify-gid:MediaImage',
      }),
      expect.objectContaining({
        path: '$.productCreateMedia.product.media.nodes[0].id',
        matcher: 'shopify-gid:MediaImage',
      }),
      expect.objectContaining({
        path: '$.product.media.nodes[0].id',
        matcher: 'shopify-gid:MediaImage',
      }),
    ]);
    expect(existsSync(resolve(repoRoot, 'config/parity-requests/productCreateMedia-downstream-read.graphql'))).toBe(
      true,
    );
  });

  it('promotes productUpdateMedia to explicit mutation and downstream read comparisons', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const spec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/productUpdateMedia-parity-plan.json'), 'utf8'),
    ) as ParitySpec;

    expect(spec.blocker).toBeUndefined();
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
          documentPath: 'config/parity-requests/productUpdateMedia-downstream-read.graphql',
          variablesCapturePath: '$.mutation.variables',
        },
        proxyPath: '$.data',
      },
    ]);
    expect(spec.comparison?.expectedDifferences).toEqual([]);
    expect(existsSync(resolve(repoRoot, 'config/parity-requests/productUpdateMedia-downstream-read.graphql'))).toBe(
      true,
    );
  });
});
