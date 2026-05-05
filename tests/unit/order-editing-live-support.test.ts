import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { listOperationRegistryEntries } from '../../scripts/support/operation-registry.js';

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
    const registry = listOperationRegistryEntries() as OperationRegistryEntry[];
    const weirdNotes = readFileSync(resolve(repoRoot, 'docs/hard-and-weird-notes.md'), 'utf8');

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orderEditBegin',
        domain: 'orders',
        execution: 'stage-locally',
        implemented: true,
        runtimeTests: ['test/parity_test.gleam'],
      }),
    );

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orderEditAddVariant',
        domain: 'orders',
        execution: 'stage-locally',
        implemented: true,
        runtimeTests: ['test/parity_test.gleam'],
      }),
    );

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orderEditSetQuantity',
        domain: 'orders',
        execution: 'stage-locally',
        implemented: true,
        runtimeTests: ['test/parity_test.gleam'],
      }),
    );

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orderEditCommit',
        domain: 'orders',
        execution: 'stage-locally',
        implemented: true,
        runtimeTests: ['test/parity_test.gleam'],
      }),
    );

    expect(weirdNotes).toContain('orderEditBegin');
    expect(weirdNotes).toContain('orderEditAddVariant');
    expect(weirdNotes).toContain('orderEditSetQuantity');
    expect(weirdNotes).toContain('orderEditCommit');
    expect(weirdNotes).toContain('Variable $id of type ID! was provided invalid value');
  });
});
