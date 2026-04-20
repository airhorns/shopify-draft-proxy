import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

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

type ParitySpec = {
  scenarioId: string;
  proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
};

describe('product media parity plan scaffolds', () => {
  it('declares concrete proxy request scaffolds for the staged media mutation family', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');

    for (const expected of expectedPlans) {
      const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.specPath), 'utf8')) as ParitySpec;
      expect(spec.scenarioId).toBe(expected.scenarioId);
      expect(spec.proxyRequest).toEqual({
        documentPath: expected.documentPath,
        variablesPath: expected.variablesPath,
      });

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
});