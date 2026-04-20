import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('generated operation coverage matrix reports', () => {
  it('publishes operation-level parity state, blocker, and assertion-kind summaries', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const jsonPath = resolve(repoRoot, 'docs/generated/operation-coverage-matrix.json');
    const markdownPath = resolve(repoRoot, 'docs/generated/operation-coverage-matrix.md');

    expect(existsSync(jsonPath)).toBe(true);
    expect(existsSync(markdownPath)).toBe(true);

    const matrix = JSON.parse(readFileSync(jsonPath, 'utf8')) as {
      totals?: {
        operations?: number;
        coveredOperations?: number;
        declaredGaps?: number;
        readyScenarios?: number;
        blockedScenarios?: number;
        pendingScenarios?: number;
      };
      blockedOperationNames?: string[];
      operations?: Array<{
        name?: string;
        type?: string;
        execution?: string;
        conformanceStatus?: string;
        assertionKinds?: string[];
        parity?: {
          readyScenarioCount?: number;
          blockedScenarioCount?: number;
          pendingScenarioCount?: number;
          hasBlockedScenario?: boolean;
        };
        scenarios?: Array<{
          id?: string;
          state?: string;
          blocker?: {
            kind?: string;
            blockerPath?: string;
          } | null;
        }>;
      }>;
    };
    const markdown = readFileSync(markdownPath, 'utf8');

    expect(matrix.totals).toEqual(
      expect.objectContaining({
        operations: 67,
        coveredOperations: 67,
        declaredGaps: 0,
        scenarios: 102,
        readyScenarios: 93,
        blockedScenarios: 9,
        pendingScenarios: 0,
        blockedOperations: 9,
        pendingOperations: 0,
      }),
    );
    expect(matrix.blockedOperationNames).toEqual([
      'productPublish',
      'productUnpublish',
      'fulfillmentTrackingInfoUpdate',
      'fulfillmentCancel',
      'draftOrderComplete',
      'orderEditBegin',
      'orderEditAddVariant',
      'orderEditSetQuantity',
      'orderEditCommit',
    ]);

    const customerCreate = matrix.operations?.find((operation) => operation.name === 'customerCreate');
    const products = matrix.operations?.find((operation) => operation.name === 'products');
    const productPublish = matrix.operations?.find((operation) => operation.name === 'productPublish');
    const inventoryActivate = matrix.operations?.find((operation) => operation.name === 'inventoryActivate');
    const orderUpdate = matrix.operations?.find((operation) => operation.name === 'orderUpdate');
    const orderCreate = matrix.operations?.find((operation) => operation.name === 'orderCreate');
    const draftOrderComplete = matrix.operations?.find((operation) => operation.name === 'draftOrderComplete');

    expect(customerCreate).toEqual(
      expect.objectContaining({
        name: 'customerCreate',
        type: 'mutation',
        execution: 'stage-locally',
        conformanceStatus: 'covered',
        assertionKinds: expect.arrayContaining(['payload-shape', 'user-errors-parity', 'downstream-read-parity']),
        parity: expect.objectContaining({
          readyScenarioCount: 1,
          blockedScenarioCount: 0,
          pendingScenarioCount: 0,
          hasBlockedScenario: false,
        }),
      }),
    );
    expect(customerCreate?.scenarios).toEqual([
      expect.objectContaining({
        id: 'customer-create-live-parity',
        state: 'ready-for-comparison',
        blocker: null,
      }),
    ]);

    expect(products).toEqual(
      expect.objectContaining({
        name: 'products',
        type: 'query',
        execution: 'overlay-read',
        conformanceStatus: 'covered',
        assertionKinds: expect.arrayContaining([
          'payload-shape',
          'selected-fields',
          'search-filter-semantics',
          'sort-order-semantics',
          'pagination-shape',
        ]),
        parity: expect.objectContaining({
          readyScenarioCount: 9,
          blockedScenarioCount: 0,
          pendingScenarioCount: 0,
          hasBlockedScenario: false,
        }),
      }),
    );
    expect(products?.scenarios).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'products-advanced-search-read',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          id: 'products-or-precedence-read',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          id: 'products-sort-keys-read',
          state: 'ready-for-comparison',
          blocker: null,
        }),
      ]),
    );

    expect(productPublish).toEqual(
      expect.objectContaining({
        name: 'productPublish',
        parity: expect.objectContaining({
          readyScenarioCount: 1,
          blockedScenarioCount: 1,
          pendingScenarioCount: 0,
          hasBlockedScenario: true,
        }),
      }),
    );
    const productPublishReadyScenario = productPublish?.scenarios?.find(
      (scenario) => scenario.id === 'productPublish-parity-plan',
    );
    const productPublishBlockedScenario = productPublish?.scenarios?.find(
      (scenario) => scenario.id === 'productPublish-aggregate-parity-blocker',
    );

    expect(productPublishReadyScenario).toEqual(
      expect.objectContaining({
        id: 'productPublish-parity-plan',
        state: 'ready-for-comparison',
        blocker: null,
      }),
    );
    expect(productPublishBlockedScenario).toEqual(
      expect.objectContaining({
        id: 'productPublish-aggregate-parity-blocker',
        state: 'blocked-with-proxy-request',
        blocker: expect.objectContaining({
          kind: 'missing-publication-target',
          blockerPath: 'pending/product-publication-conformance-scope-blocker.md',
        }),
      }),
    );

    expect(inventoryActivate).toEqual(
      expect.objectContaining({
        name: 'inventoryActivate',
        parity: expect.objectContaining({
          readyScenarioCount: 1,
          blockedScenarioCount: 0,
          pendingScenarioCount: 0,
          hasBlockedScenario: false,
        }),
      }),
    );
    expect(inventoryActivate?.scenarios).toEqual([
      expect.objectContaining({
        id: 'inventory-activate-live-parity',
        state: 'ready-for-comparison',
        blocker: null,
      }),
    ]);

    expect(orderUpdate).toEqual(
      expect.objectContaining({
        name: 'orderUpdate',
        type: 'mutation',
        execution: 'stage-locally',
        conformanceStatus: 'covered',
        assertionKinds: expect.arrayContaining(['payload-shape', 'user-errors-parity', 'selected-fields', 'downstream-read-parity']),
        parity: expect.objectContaining({
          readyScenarioCount: 5,
          blockedScenarioCount: 0,
          pendingScenarioCount: 0,
          hasBlockedScenario: false,
        }),
      }),
    );
    expect(orderUpdate?.scenarios).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'order-update-inline-missing-id-argument-error',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          id: 'order-update-inline-null-id-argument-error',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          id: 'order-update-missing-id-invalid-variable',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          id: 'order-update-unknown-id-parity',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          id: 'order-update-live-parity',
          state: 'ready-for-comparison',
          blocker: null,
        }),
      ]),
    );

    expect(orderCreate).toEqual(
      expect.objectContaining({
        name: 'orderCreate',
        type: 'mutation',
        execution: 'stage-locally',
        conformanceStatus: 'covered',
        parity: expect.objectContaining({
          readyScenarioCount: 4,
          blockedScenarioCount: 0,
          pendingScenarioCount: 0,
          hasBlockedScenario: false,
        }),
      }),
    );
    expect(orderCreate?.scenarios).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'order-create-inline-missing-order-argument-error',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          id: 'order-create-inline-null-order-argument-error',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          id: 'order-create-missing-order-invalid-variable',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          id: 'order-create-live-parity',
          state: 'ready-for-comparison',
          blocker: null,
        }),
      ]),
    );

    expect(draftOrderComplete).toEqual(
      expect.objectContaining({
        name: 'draftOrderComplete',
        type: 'mutation',
        execution: 'stage-locally',
        conformanceStatus: 'covered',
        parity: expect.objectContaining({
          readyScenarioCount: 3,
          blockedScenarioCount: 1,
          pendingScenarioCount: 0,
          hasBlockedScenario: true,
        }),
      }),
    );
    expect(draftOrderComplete?.scenarios).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'draft-order-complete-inline-missing-id-argument-error',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          id: 'draft-order-complete-inline-null-id-argument-error',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          id: 'draft-order-complete-missing-id-invalid-variable',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          id: 'draft-order-complete-live-parity',
          state: 'blocked-with-proxy-request',
          blocker: expect.objectContaining({
            kind: 'missing-live-draft-order-complete-access',
            blockerPath: 'pending/order-creation-conformance-scope-blocker.md',
          }),
        }),
      ]),
    );

    expect(markdown).toContain('# Operation Coverage Matrix');
    expect(markdown).toContain('- Operations: 67');
    expect(markdown).toContain('- Covered operations: 67');
    expect(markdown).toContain('- Declared gaps: 0');
    expect(markdown).toContain('- Ready scenarios: 93');
    expect(markdown).toContain('- Blocked scenarios: 9');
    expect(markdown).toContain('- Blocked operations: 9');
    expect(markdown).toContain('## Blocked operations');
    expect(markdown).toContain('`productPublish`');
    expect(markdown).toContain('`productUnpublish`');
    expect(markdown).not.toContain('- `orderCreate` →');
    expect(markdown).toContain('`draftOrderComplete`');
    expect(markdown).toContain('`orderEditBegin`');
    expect(markdown).not.toContain('`inventoryActivate` → `single-location-store`');
    expect(markdown).not.toContain('single-location-store');
    expect(markdown).toContain('missing-publication-target');
    expect(markdown).toContain('user-errors-parity');
    expect(markdown).toContain('customer-create-live-parity');
    expect(markdown).toContain('order-update-unknown-id-parity');
  });
});
