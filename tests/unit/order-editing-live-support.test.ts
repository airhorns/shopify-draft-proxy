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
  it('keeps the first four order-edit roots as declared gaps while the notes stay anchored to captured validation evidence', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = listOperationRegistryEntries() as OperationRegistryEntry[];
    const weirdNotes = readFileSync(resolve(repoRoot, 'docs/hard-and-weird-notes.md'), 'utf8');

    for (const name of ['orderEditBegin', 'orderEditAddVariant', 'orderEditSetQuantity', 'orderEditCommit']) {
      expect(registry).toContainEqual(
        expect.objectContaining({
          name,
          domain: 'orders',
          execution: 'stage-locally',
          implemented: false,
          runtimeTests: [],
        }),
      );
      expect(weirdNotes).toContain(name);
    }

    expect(weirdNotes).toContain('Variable $id of type ID! was provided invalid value');
  });
});
