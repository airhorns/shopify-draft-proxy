import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import type { ParitySpec } from '../../scripts/conformance-parity-lib.js';

import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';

type ScenarioRegistryEntry = {
  id: string;
  operationNames: string[];
  status: string;
  captureFiles: string[];
  paritySpecPath: string;
  notes?: string;
};

type OperationRegistryEntry = {
  name: string;
};

const expectedCoveredFamilies = [
  {
    operationName: 'productPublish',
    scenarioId: 'productPublish-parity-plan',
    paritySpecPath: 'config/parity-specs/productPublish-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-publish-parity.json',
  },
  {
    operationName: 'productUnpublish',
    scenarioId: 'productUnpublish-parity-plan',
    paritySpecPath: 'config/parity-specs/productUnpublish-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-unpublish-parity.json',
  },
] as const;

const expectedReadyFamilies = [expectedCoveredFamilies[0]] as const;
const expectedNoTargetFamilies = [expectedCoveredFamilies[1]] as const;

const expectedBlockedFamilies = [
  {
    scenarioId: 'productPublish-aggregate-parity-blocker',
    operationName: 'productPublish',
    paritySpecPath: 'config/parity-specs/productPublish-aggregate-parity-blocker.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-publish-parity.json',
  },
  {
    scenarioId: 'productUnpublish-aggregate-parity-blocker',
    operationName: 'productUnpublish',
    paritySpecPath: 'config/parity-specs/productUnpublish-aggregate-parity-blocker.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-unpublish-parity.json',
  },
] as const;

const repoRoot = resolve(import.meta.dirname, '../..');
const scenarioRegistry = loadConformanceScenarios(repoRoot) as ScenarioRegistryEntry[];
const operationRegistry = JSON.parse(
  readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
) as OperationRegistryEntry[];
const blockerNotePath = resolve(repoRoot, 'pending/product-publication-conformance-scope-blocker.md');

describe('product publication live parity promotion state', () => {
  it('marks productPublish and productUnpublish covered by captured minimal live mutation evidence', () => {
    for (const expected of expectedCoveredFamilies) {
      expect(operationRegistry).toContainEqual(
        expect.objectContaining({
          name: expected.operationName,
        }),
      );

      expect(scenarioRegistry).toContainEqual(
        expect.objectContaining({
          id: expected.scenarioId,
          operationNames: [expected.operationName],
          status: 'captured',
          paritySpecPath: expected.paritySpecPath,
          captureFiles: [expected.captureFile],
          notes: expect.stringContaining('minimal live payload slice'),
        }),
      );
    }
  });

  it('promotes productPublish to executed strict-json comparison while keeping remaining no-target captures out of parity execution', () => {
    for (const expected of expectedReadyFamilies) {
      const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.paritySpecPath), 'utf8')) as ParitySpec;
      expect(spec).toMatchObject({
        scenarioId: expected.scenarioId,
        scenarioStatus: 'captured',
        liveCaptureFiles: [expected.captureFile],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          variablesCapturePath: '$.mutation.variables',
        },
        comparison: {
          mode: 'strict-json',
          expectedDifferences: [],
          targets: [
            {
              name: 'mutation-data',
              capturePath: '$.mutation.response.data',
              proxyPath: '$.data',
            },
          ],
        },
      });

      expect(spec.blocker).toBeUndefined();
      expect(spec.notes).toContain('minimal live payload slice');
    }

    for (const expected of expectedNoTargetFamilies) {
      const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.paritySpecPath), 'utf8')) as ParitySpec;
      expect(spec).toMatchObject({
        scenarioId: expected.scenarioId,
        scenarioStatus: 'captured',
        liveCaptureFiles: [expected.captureFile],
        comparisonMode: 'captured-vs-proxy-request',
      });

      expect(spec.blocker).toEqual({
        kind: 'explicit-comparison-targets-needed',
        blockerPath: null,
      });
      expect(spec.notes).toContain('minimal live payload slice');
    }
  });

  it('keeps the aggregate publication-field blocker as a separate captured scenario per mutation root', () => {
    for (const expected of expectedBlockedFamilies) {
      expect(scenarioRegistry).toContainEqual(
        expect.objectContaining({
          id: expected.scenarioId,
          operationNames: [expected.operationName],
          status: 'captured',
          paritySpecPath: expected.paritySpecPath,
          captureFiles: [expected.captureFile],
          notes: expect.stringContaining('aggregate publication-field blocker'),
        }),
      );

      const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.paritySpecPath), 'utf8')) as ParitySpec;
      expect(spec).toMatchObject({
        scenarioId: expected.scenarioId,
        scenarioStatus: 'captured',
        liveCaptureFiles: [expected.captureFile],
        comparisonMode: 'captured-vs-proxy-request',
        blocker: {
          kind: 'missing-publication-target',
          blockerPath: 'pending/product-publication-conformance-scope-blocker.md',
          details: {
            blockedFields: ['publishedOnCurrentPublication', 'availablePublicationsCount', 'resourcePublicationsCount'],
            blockedMutations: ['productPublish', 'productUnpublish'],
            appConfigPath: '/tmp/shopify-conformance-app/hermes-conformance-products/shopify.app.toml',
            appId: '0db6d7e08e4ba05ce97440df36c7ed33',
            appHandle: 'hermes-conformance-products',
            publicationTargetStatus: 'app-missing-publication',
            publicationTargetMessage: "Your app doesn't have a publication for this shop.",
            shopifyAppCliAuthStatus: 'available',
            shopifyAppCliAuthWorkdir: '/tmp/shopify-conformance-app/hermes-conformance-products',
            shopifyAppDeployStatus: 'deployed-but-app-still-lacks-publication',
            shopifyAppDeployCommand: 'corepack pnpm exec shopify app deploy --allow-updates',
            channelConfigExtensionPath:
              '/tmp/shopify-conformance-app/hermes-conformance-products/extensions/conformance-publication-target/shopify.extension.toml',
            channelConfigHandle: 'conformance-publication-target',
            channelConfigCreateLegacyChannelOnAppInstall: true,
            publicationTargetRemediation: 'channel-config-change-still-needs-reinstall',
            activeCredentialTokenFamily: 'shpca',
            activeCredentialHeaderMode: 'raw-x-shopify-access-token',
            activeCredentialSummary: expect.stringContaining('shpca'),
          },
        },
      });

      expect(spec.notes).toContain('aggregate publication-field blocker');
      expect(spec.blocker?.details?.shopifyAppDeployVersion).toMatch(/^hermes-conformance-products-\d+$/);
    }
  });

  it('records the current publication aggregate-field blocker note alongside the now-captured mutation family', () => {
    expect(existsSync(blockerNotePath)).toBe(true);

    const blockerNote = readFileSync(blockerNotePath, 'utf8');
    expect(blockerNote).toContain('corepack pnpm conformance:capture-product-publications');
    expect(blockerNote).toContain("Your app doesn't have a publication for this shop.");
    expect(blockerNote).toContain(
      'minimal `productPublish` / `productUnpublish` mutation payloads now capture successfully',
    );
    expect(blockerNote).toContain('aggregate publication fields remain blocked');
    expect(blockerNote).toContain('corepack pnpm exec shopify app deploy --allow-updates');
    expect(blockerNote).toContain('publication aggregate reads still fail for this app');
    expect(blockerNote).toContain('conformance-publication-target/shopify.extension.toml');
    expect(blockerNote).toContain('current conformance credential family: `shpca`');
    expect(blockerNote).toContain('header mode: raw `X-Shopify-Access-Token`');
    expect(blockerNote).toContain('create_legacy_channel_on_app_install = `true`');
    expect(blockerNote).toContain('do not assume deploy alone backfills a publication on the existing store install');
    expect(blockerNote).toContain('productPublish');
    expect(blockerNote).toContain('productUnpublish');
  });
});
