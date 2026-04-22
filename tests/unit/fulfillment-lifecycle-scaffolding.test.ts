import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import type { ParitySpec } from '../../scripts/conformance-parity-lib.js';

import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';

type OperationRegistryEntry = {
  name: string;
  domain: string;
  execution: string;
  implemented: boolean;
  runtimeTests?: string[];
};

type ConformanceScenario = {
  id: string;
  operationNames: string[];
  status: string;
  captureFiles: string[];
  paritySpecPath: string;
};

const expectedRegistryEntries = [
  {
    name: 'fulfillmentTrackingInfoUpdate',
    execution: 'stage-locally',
    implemented: true,
    conformanceStatus: 'covered',
    scenarioIds: [
      'fulfillment-tracking-info-update-inline-missing-id-argument-error',
      'fulfillment-tracking-info-update-inline-null-id-argument-error',
      'fulfillment-tracking-info-update-missing-id-invalid-variable',
    ],
  },
  {
    name: 'fulfillmentCancel',
    execution: 'stage-locally',
    implemented: true,
    conformanceStatus: 'covered',
    scenarioIds: [
      'fulfillment-cancel-inline-missing-id-argument-error',
      'fulfillment-cancel-inline-null-id-argument-error',
      'fulfillment-cancel-missing-id-invalid-variable',
    ],
  },
] as const;

const expectedCapturedScenarios = [
  {
    id: 'fulfillment-tracking-info-update-inline-missing-id-argument-error',
    operationName: 'fulfillmentTrackingInfoUpdate',
    paritySpecPath: 'config/parity-specs/fulfillmentTrackingInfoUpdate-inline-missing-id-parity.json',
    documentPath: 'config/parity-requests/fulfillmentTrackingInfoUpdate-inline-missing-id-parity.graphql',
    variablesPath: 'config/parity-requests/fulfillmentTrackingInfoUpdate-inline-missing-id-parity.variables.json',
    requiredText:
      'mutation FulfillmentTrackingInfoUpdateInlineMissingId($trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean)',
    expectedVariables: {
      trackingInfoInput: {
        number: 'HERMES-TRACK-UPDATE',
        url: 'https://example.com/track/HERMES-TRACK-UPDATE',
        company: 'Hermes',
      },
      notifyCustomer: false,
    },
    fixturePath:
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/fulfillment-tracking-info-update-inline-missing-id.json',
    fixtureMessageIncludes: 'missing required arguments: fulfillmentId',
  },
  {
    id: 'fulfillment-tracking-info-update-inline-null-id-argument-error',
    operationName: 'fulfillmentTrackingInfoUpdate',
    paritySpecPath: 'config/parity-specs/fulfillmentTrackingInfoUpdate-inline-null-id-parity.json',
    documentPath: 'config/parity-requests/fulfillmentTrackingInfoUpdate-inline-null-id-parity.graphql',
    variablesPath: 'config/parity-requests/fulfillmentTrackingInfoUpdate-inline-null-id-parity.variables.json',
    requiredText:
      'mutation FulfillmentTrackingInfoUpdateInlineNullId($trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean)',
    expectedVariables: {
      trackingInfoInput: {
        number: 'HERMES-TRACK-UPDATE',
        url: 'https://example.com/track/HERMES-TRACK-UPDATE',
        company: 'Hermes',
      },
      notifyCustomer: false,
    },
    fixturePath:
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/fulfillment-tracking-info-update-inline-null-id.json',
    fixtureMessageIncludes: "invalid value (null). Expected type 'ID!'",
  },
  {
    id: 'fulfillment-tracking-info-update-missing-id-invalid-variable',
    operationName: 'fulfillmentTrackingInfoUpdate',
    paritySpecPath: 'config/parity-specs/fulfillmentTrackingInfoUpdate-missing-id-parity.json',
    documentPath: 'config/parity-requests/fulfillmentTrackingInfoUpdate-missing-id-parity.graphql',
    variablesPath: 'config/parity-requests/fulfillmentTrackingInfoUpdate-missing-id-parity.variables.json',
    requiredText:
      'mutation FulfillmentTrackingInfoUpdateMissingId($fulfillmentId: ID!, $trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean)',
    expectedVariables: {
      trackingInfoInput: {
        number: 'HERMES-TRACK-UPDATE',
        url: 'https://example.com/track/HERMES-TRACK-UPDATE',
        company: 'Hermes',
      },
      notifyCustomer: false,
    },
    fixturePath:
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/fulfillment-tracking-info-update-missing-id.json',
    fixtureMessageIncludes: 'Variable $fulfillmentId of type ID! was provided invalid value',
  },
  {
    id: 'fulfillment-cancel-inline-missing-id-argument-error',
    operationName: 'fulfillmentCancel',
    paritySpecPath: 'config/parity-specs/fulfillmentCancel-inline-missing-id-parity.json',
    documentPath: 'config/parity-requests/fulfillmentCancel-inline-missing-id-parity.graphql',
    variablesPath: 'config/parity-requests/fulfillmentCancel-inline-missing-id-parity.variables.json',
    requiredText: 'mutation FulfillmentCancelInlineMissingId',
    expectedVariables: {},
    fixturePath:
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/fulfillment-cancel-inline-missing-id.json',
    fixtureMessageIncludes: 'missing required arguments: id',
  },
  {
    id: 'fulfillment-cancel-inline-null-id-argument-error',
    operationName: 'fulfillmentCancel',
    paritySpecPath: 'config/parity-specs/fulfillmentCancel-inline-null-id-parity.json',
    documentPath: 'config/parity-requests/fulfillmentCancel-inline-null-id-parity.graphql',
    variablesPath: 'config/parity-requests/fulfillmentCancel-inline-null-id-parity.variables.json',
    requiredText: 'mutation FulfillmentCancelInlineNullId',
    expectedVariables: {},
    fixturePath:
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/fulfillment-cancel-inline-null-id.json',
    fixtureMessageIncludes: "invalid value (null). Expected type 'ID!'",
  },
  {
    id: 'fulfillment-cancel-missing-id-invalid-variable',
    operationName: 'fulfillmentCancel',
    paritySpecPath: 'config/parity-specs/fulfillmentCancel-missing-id-parity.json',
    documentPath: 'config/parity-requests/fulfillmentCancel-missing-id-parity.graphql',
    variablesPath: 'config/parity-requests/fulfillmentCancel-missing-id-parity.variables.json',
    requiredText: 'mutation FulfillmentCancelMissingId($id: ID!)',
    expectedVariables: {},
    fixturePath: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/fulfillment-cancel-missing-id.json',
    fixtureMessageIncludes: 'Variable $id of type ID! was provided invalid value',
  },
] as const;

const expectedBlockedScenarios = [
  {
    id: 'fulfillment-tracking-info-update-live-parity',
    operationName: 'fulfillmentTrackingInfoUpdate',
    paritySpecPath: 'config/parity-specs/fulfillmentTrackingInfoUpdate-parity-plan.json',
    documentPath: 'config/parity-requests/fulfillmentTrackingInfoUpdate-parity-plan.graphql',
    variablesPath: 'config/parity-requests/fulfillmentTrackingInfoUpdate-parity-plan.variables.json',
    blockerKind: 'missing-live-fulfillment-tracking-update-access',
    probeRoots: ['fulfillmentTrackingInfoUpdate'],
    requiredScopes: [
      'write_assigned_fulfillment_orders',
      'write_merchant_managed_fulfillment_orders',
      'write_third_party_fulfillment_orders',
    ],
    requiredPermissions: ['fulfill_and_ship_orders'],
    requiredText:
      'mutation FulfillmentTrackingInfoUpdateParityPlan($fulfillmentId: ID!, $trackingInfoInput: FulfillmentTrackingInput!, $notifyCustomer: Boolean)',
    failingMessageIncludes: 'fulfill and ship orders',
    expectedVariables: {
      fulfillmentId: 'gid://shopify/Fulfillment/0',
      notifyCustomer: false,
      trackingInfoInput: {
        number: 'HERMES-TRACK-UPDATE',
        url: 'https://example.com/track/HERMES-TRACK-UPDATE',
        company: 'Hermes',
      },
    },
  },
  {
    id: 'fulfillment-cancel-live-parity',
    operationName: 'fulfillmentCancel',
    paritySpecPath: 'config/parity-specs/fulfillmentCancel-parity-plan.json',
    documentPath: 'config/parity-requests/fulfillmentCancel-parity-plan.graphql',
    variablesPath: 'config/parity-requests/fulfillmentCancel-parity-plan.variables.json',
    blockerKind: 'missing-live-fulfillment-cancel-access',
    probeRoots: ['fulfillmentCancel'],
    requiredScopes: [],
    requiredPermissions: [],
    requiredText: 'mutation FulfillmentCancelParityPlan($id: ID!)',
    failingMessageIncludes: 'Access denied',
    expectedVariables: {
      id: 'gid://shopify/Fulfillment/0',
    },
  },
] as const;

describe('fulfillment lifecycle scaffolding', () => {
  it('keeps the order capture harness aligned with fulfillment lifecycle blocker refreshes', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const captureScript = readFileSync(resolve(repoRoot, 'scripts/capture-order-conformance.mts'), 'utf8');
    expect(captureScript).toContain('fulfillmentCreateInvalidIdFixturePath');
    expect(captureScript).toContain('fulfillmentTrackingInfoUpdateInlineMissingIdFixturePath');
    expect(captureScript).toContain('fulfillmentTrackingInfoUpdateInlineNullIdFixturePath');
    expect(captureScript).toContain('fulfillmentTrackingInfoUpdateMissingIdFixturePath');
    expect(captureScript).toContain('fulfillmentCancelInlineMissingIdFixturePath');
    expect(captureScript).toContain('fulfillmentCancelInlineNullIdFixturePath');
    expect(captureScript).toContain('fulfillmentCancelMissingIdFixturePath');
    expect(captureScript).toContain('fulfillmentLifecycleBlockerNotePath');
    expect(captureScript).toContain('fulfillmentTrackingInfoUpdateInlineMissingIdProbe');
    expect(captureScript).toContain('fulfillmentTrackingInfoUpdateInlineNullIdProbe');
    expect(captureScript).toContain('fulfillmentTrackingInfoUpdateMissingIdProbe');
    expect(captureScript).toContain('fulfillmentTrackingInfoUpdateProbe');
    expect(captureScript).toContain('fulfillmentTrackingInfoUpdateInlineMissingIdResult');
    expect(captureScript).toContain('fulfillmentTrackingInfoUpdateInlineNullIdResult');
    expect(captureScript).toContain('fulfillmentTrackingInfoUpdateMissingIdResult');
    expect(captureScript).toContain('fulfillmentTrackingInfoUpdateResult');
    expect(captureScript).toContain('fulfillmentCancelInlineMissingIdProbe');
    expect(captureScript).toContain('fulfillmentCancelInlineNullIdProbe');
    expect(captureScript).toContain('fulfillmentCancelMissingIdProbe');
    expect(captureScript).toContain('fulfillmentCancelProbe');
    expect(captureScript).toContain('fulfillmentCancelInlineMissingIdResult');
    expect(captureScript).toContain('fulfillmentCancelInlineNullIdResult');
    expect(captureScript).toContain('fulfillmentCancelMissingIdResult');
    expect(captureScript).toContain('fulfillmentCancelResult');
    expect(captureScript).toContain('Captured pre-access validation slices');
    expect(captureScript).toContain('Fulfillment lifecycle conformance blocker');
    expect(captureScript).toContain('corepack pnpm conformance:capture-orders');
  });

  it('tracks the next fulfillment lifecycle roots explicitly instead of leaving them as informal notes', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];

    for (const expected of expectedRegistryEntries) {
      expect(registry).toContainEqual(
        expect.objectContaining({
          name: expected.name,
          domain: 'orders',
          execution: expected.execution,
          implemented: expected.implemented,
        }),
      );
    }
  });

  it('adds captured validation scenarios plus blocked happy-path parity scaffolds for the fulfillment lifecycle roots', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const scenarios = loadConformanceScenarios(repoRoot) as ConformanceScenario[];

    for (const expected of expectedCapturedScenarios) {
      expect(scenarios).toContainEqual(
        expect.objectContaining({
          id: expected.id,
          operationNames: [expected.operationName],
          status: 'captured',
          captureFiles: [expected.fixturePath],
          paritySpecPath: expected.paritySpecPath,
        }),
      );
    }

    for (const expected of expectedBlockedScenarios) {
      expect(scenarios).toContainEqual(
        expect.objectContaining({
          id: expected.id,
          operationNames: [expected.operationName],
          status: 'planned',
          captureFiles: [],
          paritySpecPath: expected.paritySpecPath,
        }),
      );
    }
  });

  it('keeps fulfillment lifecycle parity artifacts and blockers machine-readable in parity specs, docs, and blocker notes', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const blockerNote = readFileSync(
      resolve(repoRoot, 'pending/fulfillment-lifecycle-conformance-scope-blocker.md'),
      'utf8',
    );
    const weirdNotes = readFileSync(resolve(repoRoot, 'docs/hard-and-weird-notes.md'), 'utf8');

    for (const expected of expectedCapturedScenarios) {
      const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.paritySpecPath), 'utf8')) as ParitySpec;
      const document = readFileSync(resolve(repoRoot, expected.documentPath), 'utf8');
      const variables = JSON.parse(readFileSync(resolve(repoRoot, expected.variablesPath), 'utf8')) as Record<
        string,
        unknown
      >;
      const fixture = JSON.parse(readFileSync(resolve(repoRoot, expected.fixturePath), 'utf8')) as {
        mutation?: { response?: { errors?: Array<{ message?: string }> } };
      };

      expect(spec).toEqual(
        expect.objectContaining({
          scenarioId: expected.id,
          scenarioStatus: 'captured',
          liveCaptureFiles: [expected.fixturePath],
          comparisonMode: 'captured-vs-proxy-request',
          proxyRequest: expect.objectContaining({
            documentPath: expected.documentPath,
            variablesPath: expected.variablesPath,
          }),
        }),
      );
      expect(document).toContain(expected.requiredText.split('(')[0]);
      expect(document).toContain('userErrors');
      expect(variables).toEqual(expected.expectedVariables);
      expect(fixture.mutation?.response?.errors?.[0]?.message).toContain(expected.fixtureMessageIncludes);
    }

    for (const expected of expectedBlockedScenarios) {
      const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.paritySpecPath), 'utf8')) as ParitySpec;
      const document = readFileSync(resolve(repoRoot, expected.documentPath), 'utf8');
      const variables = JSON.parse(readFileSync(resolve(repoRoot, expected.variablesPath), 'utf8')) as Record<
        string,
        unknown
      >;

      expect(spec).toEqual(
        expect.objectContaining({
          scenarioId: expected.id,
          scenarioStatus: 'planned',
          liveCaptureFiles: [],
          comparisonMode: 'planned',
          proxyRequest: expect.objectContaining({
            documentPath: expected.documentPath,
            variablesPath: expected.variablesPath,
          }),
          blocker: expect.objectContaining({
            kind: expected.blockerKind,
            blockerPath: 'pending/fulfillment-lifecycle-conformance-scope-blocker.md',
            details: expect.objectContaining({
              requiredScopes: expected.requiredScopes,
              requiredPermissions: expected.requiredPermissions,
              probeRoots: expected.probeRoots,
            }),
          }),
        }),
      );
      expect(document).toContain(expected.requiredText.split('(')[0]);
      expect(document).toContain('userErrors');
      expect(spec.blocker?.details?.failingMessage).toContain(expected.failingMessageIncludes);
      expect(spec.blocker?.details?.manualStoreAuthStatus).toBe('present-shpca-user-token-not-offline-capable');
      expect(spec.blocker?.details?.manualStoreAuthTokenPath).toBe('.manual-store-auth-token.json');
      expect(spec.blocker?.details?.manualStoreAuthCachedScopes).toContain('write_fulfillments');
      expect(spec.blocker?.details?.manualStoreAuthAssociatedUserScopes ?? []).toEqual([]);
      expect(variables).toEqual(expected.expectedVariables);
    }

    expect(blockerNote).toContain('Current run summary');
    expect(blockerNote).toContain('access denied on the current repo credential');
    expect(blockerNote).toContain('fulfillmentTrackingInfoUpdate');
    expect(blockerNote).toContain('fulfillmentCancel');
    expect(blockerNote).toContain('fulfill_and_ship_orders');
    expect(blockerNote).toContain('.manual-store-auth-token.json');
    expect(blockerNote).toContain('corepack pnpm conformance:capture-orders');

    expect(weirdNotes).toContain('fulfillmentTrackingInfoUpdate');
    expect(weirdNotes).toContain('fulfillmentCancel');
    expect(weirdNotes).toContain('fulfill_and_ship_orders');
  });
});
