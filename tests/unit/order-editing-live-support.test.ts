import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

type OperationRegistryEntry = {
  name: string;
  domain: string;
  execution: string;
  implemented: boolean;
  runtimeTests?: string[];
};

describe('order editing live support registry/docs', () => {
  it('promotes the first four order-edit roots to covered once captured missing-id GraphQL validation branches land and keeps the order-edit notes anchored to that evidence', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];
    const weirdNotes = readFileSync(resolve(repoRoot, 'docs/hard-and-weird-notes.md'), 'utf8');

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orderEditBegin',
        domain: 'orders',
        execution: 'stage-locally',
        implemented: true,
        runtimeTests: ['tests/integration/order-edit-flow.test.ts'],
      }),
    );

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orderEditAddVariant',
        domain: 'orders',
        execution: 'stage-locally',
        implemented: true,
        runtimeTests: ['tests/integration/order-edit-flow.test.ts'],
      }),
    );

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orderEditSetQuantity',
        domain: 'orders',
        execution: 'stage-locally',
        implemented: true,
        runtimeTests: ['tests/integration/order-edit-flow.test.ts'],
      }),
    );

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orderEditCommit',
        domain: 'orders',
        execution: 'stage-locally',
        implemented: true,
        runtimeTests: ['tests/integration/order-edit-flow.test.ts'],
      }),
    );

    expect(weirdNotes).toContain('orderEditBegin');
    expect(weirdNotes).toContain('orderEditAddVariant');
    expect(weirdNotes).toContain('orderEditSetQuantity');
    expect(weirdNotes).toContain('orderEditCommit');
    expect(weirdNotes).toContain('Variable $id of type ID! was provided invalid value');
  });
});
