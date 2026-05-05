import { execFileSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  listImplementedOperationRegistryEntries,
  listOperationRegistryEntries,
} from '../../scripts/support/operation-registry.js';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

describe('operation registry', () => {
  it('keeps implemented capability names unique', () => {
    const implementedNames = listImplementedOperationRegistryEntries().map((entry) => entry.name);
    expect(new Set(implementedNames).size).toBe(implementedNames.length);
  });

  it('requires implemented operations to declare runtime tests without conformance metadata', () => {
    for (const entry of listImplementedOperationRegistryEntries()) {
      expect(entry.runtimeTests.length).toBeGreaterThan(0);
      expect('conformance' in entry).toBe(false);
    }
  });

  it('keeps implemented runtime test references executable on disk', () => {
    for (const entry of listImplementedOperationRegistryEntries()) {
      for (const runtimeTest of entry.runtimeTests) {
        expect(() => {
          execFileSync('test', ['-f', runtimeTest], {
            cwd: repoRoot,
            stdio: 'pipe',
          });
        }, `${entry.name} runtime test should exist: ${runtimeTest}`).not.toThrow();
      }
    }
  });

  it('exposes both overlay-read and stage-locally implemented operations', () => {
    const executions = new Set(listOperationRegistryEntries().map((entry) => entry.execution));
    expect(executions.has('overlay-read')).toBe(true);
    expect(executions.has('stage-locally')).toBe(true);
  });

  it('keeps the generated Gleam operation registry mirror in sync', () => {
    expect(() => {
      execFileSync('bash', ['scripts/sync-operation-registry.sh', '--check'], {
        cwd: repoRoot,
        encoding: 'utf8',
      });
    }).not.toThrow();
  });
});
