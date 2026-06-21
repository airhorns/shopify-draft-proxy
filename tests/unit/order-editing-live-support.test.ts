import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { loadOperationRegistry } from '../../scripts/conformance-scenario-registry.js';

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
    const registry = loadOperationRegistry(repoRoot) as OperationRegistryEntry[];
    const weirdNotes = readFileSync(resolve(repoRoot, 'docs/hard-and-weird-notes.md'), 'utf8');

    for (const name of ['orderEditAddVariant', 'orderEditSetQuantity']) {
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

    // orderEditBegin and orderEditCommit are now implemented locally
    for (const name of ['orderEditBegin', 'orderEditCommit']) {
      expect(registry).toContainEqual(
        expect.objectContaining({
          name,
          domain: 'orders',
          execution: 'stage-locally',
          implemented: true,
        }),
      );
    }

    expect(weirdNotes).toContain('orderEditBegin');
    expect(weirdNotes).toContain('orderEditCommit');

    expect(weirdNotes).toContain('Variable $id of type ID! was provided invalid value');
  });
});
