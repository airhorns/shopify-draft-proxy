import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

type OperationRegistryEntry = {
  name: string;
  domain: string;
  execution: string;
  implemented: boolean;
  runtimeTests?: string[];
  conformance?: {
    status?: string;
    reason?: string;
    scenarioIds?: string[];
  };
};

describe('order editing live support registry/docs', () => {
  it('promotes the first four order-edit roots to covered once captured missing-id GraphQL validation branches land while keeping the happy-path blocker family honest', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8')) as OperationRegistryEntry[];
    const worklist = readFileSync(resolve(repoRoot, 'docs/shopify-admin-worklist.md'), 'utf8');
    const weirdNotes = readFileSync(resolve(repoRoot, 'docs/hard-and-weird-notes.md'), 'utf8');

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orderEditBegin',
        domain: 'orders',
        execution: 'stage-locally',
        implemented: true,
        runtimeTests: ['tests/integration/order-edit-flow.test.ts'],
        conformance: expect.objectContaining({
          status: 'covered',
          scenarioIds: ['order-edit-begin-missing-id-invalid-variable'],
        }),
      }),
    );

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orderEditAddVariant',
        domain: 'orders',
        execution: 'stage-locally',
        implemented: true,
        runtimeTests: ['tests/integration/order-edit-flow.test.ts'],
        conformance: expect.objectContaining({
          status: 'covered',
          scenarioIds: ['order-edit-add-variant-missing-id-invalid-variable'],
        }),
      }),
    );

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orderEditSetQuantity',
        domain: 'orders',
        execution: 'stage-locally',
        implemented: true,
        runtimeTests: ['tests/integration/order-edit-flow.test.ts'],
        conformance: expect.objectContaining({
          status: 'covered',
          scenarioIds: ['order-edit-set-quantity-missing-id-invalid-variable'],
        }),
      }),
    );

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orderEditCommit',
        domain: 'orders',
        execution: 'stage-locally',
        implemented: true,
        runtimeTests: ['tests/integration/order-edit-flow.test.ts'],
        conformance: expect.objectContaining({
          status: 'covered',
          scenarioIds: ['order-edit-commit-missing-id-invalid-variable'],
        }),
      }),
    );

    expect(worklist).toContain('`orderEditBegin`');
    expect(worklist).toContain('order-edit-begin-missing-id.json');
    expect(worklist).toContain('`orderEditAddVariant`');
    expect(worklist).toContain('order-edit-add-variant-missing-id.json');
    expect(worklist).toContain('captured missing-`$id` GraphQL validation branch');
    expect(worklist).toContain('`orderEditSetQuantity`');
    expect(worklist).toContain('order-edit-set-quantity-missing-id.json');
    expect(worklist).toContain('`orderEditCommit`');
    expect(worklist).toContain('order-edit-commit-missing-id.json');
    expect(worklist).toContain('write_order_edits');
    expect(weirdNotes).toContain('orderEditBegin');
    expect(weirdNotes).toContain('orderEditAddVariant');
    expect(weirdNotes).toContain('orderEditSetQuantity');
    expect(weirdNotes).toContain('orderEditCommit');
    expect(weirdNotes).toContain('Variable $id of type ID! was provided invalid value');
    expect(weirdNotes).toContain('write_order_edits');
  });
});
