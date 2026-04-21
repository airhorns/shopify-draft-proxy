import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import type { ParitySpec } from '../../scripts/conformance-parity-lib.js';

import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';

type OperationRegistryEntry = {
  name: string;
  implemented?: boolean;
  runtimeTests?: string[];
};

type ConformanceScenario = {
  id: string;
  operationNames: string[];
  status: string;
  captureFiles: string[];
  paritySpecPath: string;
};

describe('order read scaffolding', () => {
  it('tracks the current direct-order empty-state slice plus captured draft-order detail/catalog/count reads explicitly', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'order',
        implemented: true,
        runtimeTests: ['tests/integration/order-query-shapes.test.ts'],
      }),
    );
    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orders',
        implemented: true,
        runtimeTests: ['tests/integration/order-query-shapes.test.ts'],
      }),
    );
    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'ordersCount',
        implemented: true,
        runtimeTests: ['tests/integration/order-query-shapes.test.ts'],
      }),
    );

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'draftOrder',
        implemented: true,
        runtimeTests: ['tests/integration/order-creation-flow.test.ts'],
      }),
    );
    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'draftOrders',
        implemented: true,
        runtimeTests: ['tests/integration/order-creation-flow.test.ts'],
      }),
    );
    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'draftOrdersCount',
        implemented: true,
        runtimeTests: ['tests/integration/order-creation-flow.test.ts'],
      }),
    );
  });

  it('adds conformance scenarios and parity specs for the safe order empty-state slice plus captured draft-order detail/catalog/count reads', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const scenarios = loadConformanceScenarios(repoRoot) as ConformanceScenario[];

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'order-empty-state-read',
        operationNames: ['order', 'orders', 'ordersCount'],
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-empty-state.json'],
        paritySpecPath: 'config/parity-specs/order-empty-state-read.json',
      }),
    );

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'draft-order-detail-read',
        operationNames: ['draftOrder'],
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-detail.json'],
        paritySpecPath: 'config/parity-specs/draftOrder-read-parity-plan.json',
      }),
    );

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'draft-orders-catalog-read',
        operationNames: ['draftOrders'],
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-orders-catalog.json'],
        paritySpecPath: 'config/parity-specs/draftOrders-read-parity-plan.json',
      }),
    );
    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'draft-orders-count-read',
        operationNames: ['draftOrdersCount'],
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-orders-count.json'],
        paritySpecPath: 'config/parity-specs/draftOrdersCount-read-parity-plan.json',
      }),
    );

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'draft-orders-invalid-email-query-read',
        operationNames: ['draftOrders', 'draftOrdersCount'],
        status: 'captured',
        captureFiles: [
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-orders-invalid-email-query.json',
        ],
        paritySpecPath: 'config/parity-specs/draftOrders-invalid-email-query-read.json',
      }),
    );
  });

  it('keeps order-read parity artifacts executable after promoting draft-order catalog/count reads to captured parity', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');

    const orderSpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/order-empty-state-read.json'), 'utf8'),
    ) as ParitySpec;
    expect(orderSpec).toEqual(
      expect.objectContaining({
        scenarioId: 'order-empty-state-read',
        scenarioStatus: 'captured',
        liveCaptureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-empty-state.json'],
        comparisonMode: 'capture-only',
        proxyRequest: {
          documentPath: 'config/parity-requests/order-empty-state-read.graphql',
          variablesPath: 'config/parity-requests/order-empty-state-read.variables.json',
        },
      }),
    );

    const orderDocument = readFileSync(
      resolve(repoRoot, 'config/parity-requests/order-empty-state-read.graphql'),
      'utf8',
    );
    const orderVariables = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-requests/order-empty-state-read.variables.json'), 'utf8'),
    ) as Record<string, unknown>;
    expect(orderDocument).toContain('query OrderEmptyStateRead');
    expect(orderDocument).toContain('order(id: $missingOrderId)');
    expect(orderDocument).toContain('orders(first: $first, sortKey: CREATED_AT, reverse: true)');
    expect(orderDocument).toContain('ordersCount');
    expect(orderVariables).toEqual({
      missingOrderId: 'gid://shopify/Order/0',
      first: 1,
    });

    const detailSpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/draftOrder-read-parity-plan.json'), 'utf8'),
    ) as ParitySpec;
    const detailDocument = readFileSync(
      resolve(repoRoot, 'config/parity-requests/draftOrder-read-parity-plan.graphql'),
      'utf8',
    );
    const detailVariables = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-requests/draftOrder-read-parity-plan.variables.json'), 'utf8'),
    ) as Record<string, unknown>;
    expect(detailSpec).toEqual(
      expect.objectContaining({
        scenarioId: 'draft-order-detail-read',
        scenarioStatus: 'captured',
        liveCaptureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-order-detail.json'],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          documentPath: 'config/parity-requests/draftOrder-read-parity-plan.graphql',
          variablesPath: 'config/parity-requests/draftOrder-read-parity-plan.variables.json',
        },
      }),
    );
    expect(detailDocument).toContain('query DraftOrderReadParityPlan($id: ID!)');
    expect(detailDocument).toContain('draftOrder(id: $id)');
    expect(detailDocument).toContain('invoiceUrl');
    expect(detailDocument).toContain('customAttributes');
    expect(detailDocument).toContain('lineItems(first: 5)');
    expect(detailVariables).toEqual({
      id: 'gid://shopify/DraftOrder/1305399296233',
    });

    const capturedSpecs = [
      {
        specPath: 'config/parity-specs/draftOrders-read-parity-plan.json',
        documentPath: 'config/parity-requests/draftOrders-read-parity-plan.graphql',
        variablesPath: 'config/parity-requests/draftOrders-read-parity-plan.variables.json',
        scenarioId: 'draft-orders-catalog-read',
        liveCaptureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-orders-catalog.json'],
        requiredText: 'draftOrders(first: $first, reverse: true)',
      },
      {
        specPath: 'config/parity-specs/draftOrdersCount-read-parity-plan.json',
        documentPath: 'config/parity-requests/draftOrdersCount-read-parity-plan.graphql',
        variablesPath: 'config/parity-requests/draftOrdersCount-read-parity-plan.variables.json',
        scenarioId: 'draft-orders-count-read',
        liveCaptureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-orders-count.json'],
        requiredText: 'draftOrdersCount(query: $query)',
      },
      {
        specPath: 'config/parity-specs/draftOrders-invalid-email-query-read.json',
        documentPath: 'config/parity-requests/draftOrders-invalid-email-query-read.graphql',
        variablesPath: 'config/parity-requests/draftOrders-invalid-email-query-read.variables.json',
        scenarioId: 'draft-orders-invalid-email-query-read',
        liveCaptureFiles: [
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/draft-orders-invalid-email-query.json',
        ],
        requiredText: 'draftOrders(first: $first, query: $query)',
      },
    ] as const;

    for (const captured of capturedSpecs) {
      const spec = JSON.parse(readFileSync(resolve(repoRoot, captured.specPath), 'utf8')) as ParitySpec;
      const document = readFileSync(resolve(repoRoot, captured.documentPath), 'utf8');
      const variables = JSON.parse(readFileSync(resolve(repoRoot, captured.variablesPath), 'utf8')) as Record<
        string,
        unknown
      >;

      expect(spec).toEqual(
        expect.objectContaining({
          scenarioId: captured.scenarioId,
          scenarioStatus: 'captured',
          liveCaptureFiles: captured.liveCaptureFiles,
          comparisonMode: 'captured-vs-proxy-request',
          proxyRequest: {
            documentPath: captured.documentPath,
            variablesPath: captured.variablesPath,
          },
        }),
      );
      expect(spec.blocker).toEqual({
        kind: 'explicit-comparison-targets-needed',
        blockerPath: null,
      });
      expect(document).toContain(captured.requiredText);
      expect(Object.keys(variables).length).toBeGreaterThan(0);
    }

    const blockerPath = resolve(repoRoot, 'pending/draft-order-read-conformance-scope-blocker.md');
    if (existsSync(blockerPath)) {
      const blockerNote = readFileSync(blockerPath, 'utf8');
      expect(blockerNote).toContain(
        'current run is auth-regressed before the draft-order read roots could be reprobed',
      );
      expect(blockerNote).toContain('draft-orders-catalog.json');
      expect(blockerNote).toContain('draft-orders-count.json');
    }

    const captureScript = readFileSync(resolve(repoRoot, 'scripts/capture-order-conformance.mts'), 'utf8');
    expect(captureScript).toContain(
      "const draftOrdersCatalogFixturePath = path.join(fixtureDir, 'draft-orders-catalog.json');",
    );
    expect(captureScript).toContain(
      "const draftOrdersCountFixturePath = path.join(fixtureDir, 'draft-orders-count.json');",
    );
    expect(captureScript).toContain(
      "const draftOrdersInvalidEmailQueryFixturePath = path.join(fixtureDir, 'draft-orders-invalid-email-query.json');",
    );
    expect(captureScript).toContain('const draftOrdersCatalogResult =');
    expect(captureScript).toContain('const draftOrdersCountResult =');
    expect(captureScript).toContain('const draftOrdersInvalidEmailQueryResult =');
    expect(captureScript).toContain('await rm(draftOrderReadBlockerNotePath, { force: true });');
    expect(captureScript).toContain('Draft-order read conformance blocker');
    expect(captureScript).toContain(
      'draftOrderReadBlockerNotePath: authRegressed ? draftOrderReadBlockerNotePath : null',
    );
  });
});
