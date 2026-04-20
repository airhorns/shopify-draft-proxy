import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

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

type CapturedScenario = {
  id: string;
  operationName: string;
  captureFile: string;
  paritySpecPath: string;
  documentPath: string;
  variablesPath: string;
  requiredText: string;
  expectedVariables: Record<string, unknown>;
};

type ParitySpec = {
  scenarioId: string;
  scenarioStatus: string;
  liveCaptureFiles: string[];
  comparisonMode: string;
  proxyRequest?: {
    documentPath?: string | null;
    variablesPath?: string | null;
  };
  blocker?: {
    kind?: string;
    blockerPath?: string;
    details?: {
      requiredScopes?: string[];
      probeRoots?: string[];
      failingMessage?: string;
      manualStoreAuthStatus?: string;
      manualStoreAuthTokenPath?: string;
      manualStoreAuthCachedScopes?: string[];
      manualStoreAuthAssociatedUserScopes?: string[];
    };
  };
};

const expectedOrderEditRegistryEntries = [
  {
    name: 'orderEditBegin',
    execution: 'stage-locally',
    implemented: true,
  },
  {
    name: 'orderEditAddVariant',
    execution: 'stage-locally',
    implemented: true,
  },
  {
    name: 'orderEditSetQuantity',
    execution: 'stage-locally',
    implemented: true,
  },
  {
    name: 'orderEditCommit',
    execution: 'stage-locally',
    implemented: true,
  },
] as const;

const expectedCapturedScenarios: CapturedScenario[] = [
  {
    id: 'order-edit-begin-missing-id-invalid-variable',
    operationName: 'orderEditBegin',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-edit-begin-missing-id.json',
    paritySpecPath: 'config/parity-specs/orderEditBegin-missing-id-parity.json',
    documentPath: 'config/parity-requests/orderEditBegin-missing-id-parity.graphql',
    variablesPath: 'config/parity-requests/orderEditBegin-missing-id-parity.variables.json',
    requiredText: 'mutation OrderEditBeginMissingId($id: ID!)',
    expectedVariables: {},
  },
  {
    id: 'order-edit-add-variant-missing-id-invalid-variable',
    operationName: 'orderEditAddVariant',
    captureFile:
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-edit-add-variant-missing-id.json',
    paritySpecPath: 'config/parity-specs/orderEditAddVariant-missing-id-parity.json',
    documentPath: 'config/parity-requests/orderEditAddVariant-missing-id-parity.graphql',
    variablesPath: 'config/parity-requests/orderEditAddVariant-missing-id-parity.variables.json',
    requiredText: 'mutation OrderEditAddVariantMissingId($id: ID!, $variantId: ID!, $quantity: Int!)',
    expectedVariables: {
      variantId: 'gid://shopify/ProductVariant/0',
      quantity: 1,
    },
  },
  {
    id: 'order-edit-set-quantity-missing-id-invalid-variable',
    operationName: 'orderEditSetQuantity',
    captureFile:
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-edit-set-quantity-missing-id.json',
    paritySpecPath: 'config/parity-specs/orderEditSetQuantity-missing-id-parity.json',
    documentPath: 'config/parity-requests/orderEditSetQuantity-missing-id-parity.graphql',
    variablesPath: 'config/parity-requests/orderEditSetQuantity-missing-id-parity.variables.json',
    requiredText: 'mutation OrderEditSetQuantityMissingId($id: ID!, $lineItemId: ID!, $quantity: Int!)',
    expectedVariables: {
      lineItemId: 'gid://shopify/CalculatedLineItem/0',
      quantity: 1,
    },
  },
  {
    id: 'order-edit-commit-missing-id-invalid-variable',
    operationName: 'orderEditCommit',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-edit-commit-missing-id.json',
    paritySpecPath: 'config/parity-specs/orderEditCommit-missing-id-parity.json',
    documentPath: 'config/parity-requests/orderEditCommit-missing-id-parity.graphql',
    variablesPath: 'config/parity-requests/orderEditCommit-missing-id-parity.variables.json',
    requiredText: 'mutation OrderEditCommitMissingId($id: ID!, $notifyCustomer: Boolean, $staffNote: String)',
    expectedVariables: {
      notifyCustomer: false,
      staffNote: 'missing id probe',
    },
  },
];

const expectedBlockedScenarios = [
  {
    id: 'order-edit-begin-live-parity',
    operationName: 'orderEditBegin',
    paritySpecPath: 'config/parity-specs/orderEditBegin-parity-plan.json',
    documentPath: 'config/parity-requests/orderEditBegin-parity-plan.graphql',
    variablesPath: 'config/parity-requests/orderEditBegin-parity-plan.variables.json',
    blockerKind: 'missing-live-order-edit-begin-access',
    probeRoots: ['orderEditBegin'],
    requiredScopes: ['write_order_edits'],
    requiredText: 'mutation OrderEditBeginParityPlan($id: ID!)',
    failingMessageIncludes: 'write_order_edits',
  },
  {
    id: 'order-edit-add-variant-live-parity',
    operationName: 'orderEditAddVariant',
    paritySpecPath: 'config/parity-specs/orderEditAddVariant-parity-plan.json',
    documentPath: 'config/parity-requests/orderEditAddVariant-parity-plan.graphql',
    variablesPath: 'config/parity-requests/orderEditAddVariant-parity-plan.variables.json',
    blockerKind: 'missing-live-order-edit-add-variant-access',
    probeRoots: ['orderEditAddVariant'],
    requiredScopes: ['write_order_edits'],
    requiredText: 'mutation OrderEditAddVariantParityPlan($id: ID!, $variantId: ID!, $quantity: Int!)',
    failingMessageIncludes: 'write_order_edits',
  },
  {
    id: 'order-edit-set-quantity-live-parity',
    operationName: 'orderEditSetQuantity',
    paritySpecPath: 'config/parity-specs/orderEditSetQuantity-parity-plan.json',
    documentPath: 'config/parity-requests/orderEditSetQuantity-parity-plan.graphql',
    variablesPath: 'config/parity-requests/orderEditSetQuantity-parity-plan.variables.json',
    blockerKind: 'missing-live-order-edit-set-quantity-access',
    probeRoots: ['orderEditSetQuantity'],
    requiredScopes: ['write_order_edits'],
    requiredText: 'mutation OrderEditSetQuantityParityPlan($id: ID!, $lineItemId: ID!, $quantity: Int!)',
    failingMessageIncludes: 'write_order_edits',
  },
  {
    id: 'order-edit-commit-live-parity',
    operationName: 'orderEditCommit',
    paritySpecPath: 'config/parity-specs/orderEditCommit-parity-plan.json',
    documentPath: 'config/parity-requests/orderEditCommit-parity-plan.graphql',
    variablesPath: 'config/parity-requests/orderEditCommit-parity-plan.variables.json',
    blockerKind: 'missing-live-order-edit-commit-access',
    probeRoots: ['orderEditCommit'],
    requiredScopes: ['write_order_edits'],
    requiredText: 'mutation OrderEditCommitParityPlan($id: ID!, $notifyCustomer: Boolean, $staffNote: String)',
    failingMessageIncludes: 'write_order_edits',
  },
] as const;

describe('order editing scaffolding', () => {
  it('registers the first order-editing roots with explicit blocked status instead of leaving them invisible', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];

    for (const expected of expectedOrderEditRegistryEntries) {
      expect(registry).toContainEqual(
        expect.objectContaining({
          name: expected.name,
          domain: 'orders',
          execution: expected.execution,
          implemented: expected.implemented,
          runtimeTests: ['tests/integration/order-edit-flow.test.ts'],
        }),
      );
    }
  });

  it('tracks both the captured orderEditBegin validation slice and the remaining blocked live order-edit scenarios explicitly', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const scenarios = loadConformanceScenarios(repoRoot) as ConformanceScenario[];

    for (const expected of expectedCapturedScenarios) {
      expect(scenarios).toContainEqual(
        expect.objectContaining({
          id: expected.id,
          operationNames: [expected.operationName],
          status: 'captured',
          captureFiles: [expected.captureFile],
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

  it('keeps the captured orderEditBegin missing-id parity artifacts executable', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');

    for (const expected of expectedCapturedScenarios) {
      const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.paritySpecPath), 'utf8')) as ParitySpec;
      const document = readFileSync(resolve(repoRoot, expected.documentPath), 'utf8');
      const variables = JSON.parse(readFileSync(resolve(repoRoot, expected.variablesPath), 'utf8')) as Record<
        string,
        unknown
      >;

      expect(spec).toEqual(
        expect.objectContaining({
          scenarioId: expected.id,
          scenarioStatus: 'captured',
          liveCaptureFiles: [expected.captureFile],
          comparisonMode: 'captured-vs-proxy-request',
          proxyRequest: {
            documentPath: expected.documentPath,
            variablesPath: expected.variablesPath,
          },
        }),
      );
      expect(document).toContain(expected.requiredText);
      if (expected.operationName === 'orderEditBegin') {
        expect(document).toContain('orderEditBegin(id: $id)');
      }
      if (expected.operationName === 'orderEditAddVariant') {
        expect(document).toContain('orderEditAddVariant(id: $id, variantId: $variantId, quantity: $quantity)');
        expect(document).toContain('calculatedLineItem');
      }
      expect(document).toContain('userErrors');
      expect(variables).toEqual(expected.expectedVariables);
    }
  });

  it('keeps blocked order-edit parity specs executable with concrete request scaffolds and blocker metadata', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');

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
          proxyRequest: {
            documentPath: expected.documentPath,
            variablesPath: expected.variablesPath,
          },
          blocker: expect.objectContaining({
            kind: expected.blockerKind,
            blockerPath: 'pending/order-editing-conformance-scope-blocker.md',
            details: expect.objectContaining({
              requiredScopes: expected.requiredScopes,
              probeRoots: expected.probeRoots,
            }),
          }),
        }),
      );
      expect(document).toContain(expected.requiredText);
      expect(document).toContain('userErrors');
      expect(spec.blocker?.details?.failingMessage).toContain(expected.failingMessageIncludes);
      expect(spec.blocker?.details?.manualStoreAuthStatus).toBe('present-shpca-user-token-not-offline-capable');
      expect(spec.blocker?.details?.manualStoreAuthTokenPath).toBe('.manual-store-auth-token.json');
      expect(spec.blocker?.details?.manualStoreAuthCachedScopes).toContain('write_orders');
      expect(spec.blocker?.details?.manualStoreAuthAssociatedUserScopes ?? []).toEqual([]);

      if (expected.operationName === 'orderEditBegin') {
        expect(variables).toEqual({ id: 'gid://shopify/Order/0' });
      }
      if (expected.operationName === 'orderEditAddVariant') {
        expect(variables).toEqual({
          id: 'gid://shopify/CalculatedOrder/0',
          variantId: 'gid://shopify/ProductVariant/0',
          quantity: 1,
        });
      }
      if (expected.operationName === 'orderEditSetQuantity') {
        expect(variables).toEqual({
          id: 'gid://shopify/CalculatedOrder/0',
          lineItemId: 'gid://shopify/CalculatedLineItem/0',
          quantity: 2,
        });
      }
      if (expected.operationName === 'orderEditCommit') {
        expect(variables).toEqual({
          id: 'gid://shopify/CalculatedOrder/0',
          notifyCustomer: false,
          staffNote: 'order edit commit parity plan',
        });
      }
    }
  });

  it('refreshes a dedicated blocker note with the current write_order_edits access-denied evidence', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const captureScript = readFileSync(resolve(repoRoot, 'scripts/capture-order-conformance.mjs'), 'utf8');
    const blockerNote = readFileSync(resolve(repoRoot, 'pending/order-editing-conformance-scope-blocker.md'), 'utf8');

    expect(captureScript).toContain(
      "const orderEditingBlockerNotePath = path.join('pending', 'order-editing-conformance-scope-blocker.md');",
    );
    expect(captureScript).toContain('first local calculated-order edit flow');
    expect(captureScript).not.toContain(
      'only after live write evidence exists should the proxy start staging calculated-order state',
    );

    expect(blockerNote).not.toContain('Current auth regression on this host');
    expect(blockerNote).toContain('orderEditBegin');
    expect(blockerNote).toContain('orderEditAddVariant');
    expect(blockerNote).toContain('orderEditSetQuantity');
    expect(blockerNote).toContain('orderEditCommit');
    expect(blockerNote).toContain('write_order_edits');
    expect(blockerNote).toContain('.manual-store-auth-token.json');
    expect(blockerNote).toContain('corepack pnpm conformance:capture-orders');
    expect(blockerNote).toContain('first local calculated-order edit flow');
    expect(blockerNote).toContain('the remaining gap is live Shopify parity for non-local orders');
    expect(blockerNote).not.toContain(
      'only after live write evidence exists should the proxy start staging calculated-order state',
    );
  });
});
