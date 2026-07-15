import { describe, expect, it } from 'vitest';

import {
  compareParityFailures,
  parseParityResult,
  type ParityResult,
} from '../../scripts/check-no-new-parity-failures.js';

function result(selectedSpecs: string[], failedSpecs: string[]): ParityResult {
  const failed = new Set(failedSpecs);
  return {
    schemaVersion: 1,
    selectedSpecs,
    passedSpecs: selectedSpecs.filter((specPath) => !failed.has(specPath)),
    failedSpecs: failedSpecs.map((specPath) => ({ specPath, errors: [`${specPath} mismatch`] })),
  };
}

describe('parity failure regression check', () => {
  it('allows known main failures while reporting fixes', () => {
    const baseline = result(['a.json', 'b.json', 'c.json'], ['a.json', 'b.json']);
    const current = result(['a.json', 'b.json', 'c.json'], ['b.json']);

    expect(compareParityFailures(current, baseline)).toEqual({
      baselineFailures: ['a.json', 'b.json'],
      currentFailures: ['b.json'],
      baselineFailureTargets: ['a.json', 'b.json'],
      currentFailureTargets: ['b.json'],
      missingSpecs: [],
      retiredSpecs: [],
      newlyFailingSpecs: [],
      newlyFailingTargets: [],
      resolvedSpecs: ['a.json'],
      resolvedTargets: ['a.json'],
    });
  });

  it('identifies failures absent from the main baseline, including new specs', () => {
    const baseline = result(['a.json', 'b.json'], ['a.json']);
    const current = result(['a.json', 'b.json', 'new.json'], ['a.json', 'b.json', 'new.json']);

    expect(compareParityFailures(current, baseline).newlyFailingSpecs).toEqual(['b.json', 'new.json']);
  });

  it('detects a new target inside a spec that already failed on main', () => {
    const baseline = result(['known.json'], ['known.json']);
    baseline.failedSpecs[0]!.errors = ['known.json [existing target] mismatch'];
    const current = result(['known.json'], ['known.json']);
    current.failedSpecs[0]!.errors = [
      'known.json [existing target] mismatch changed details',
      'known.json [new target] mismatch',
    ];

    expect(compareParityFailures(current, baseline).newlyFailingTargets).toEqual(['known.json [new target]']);
  });

  it('reports baseline specs omitted from the current result instead of treating them as resolved', () => {
    const baseline = result(['kept.json', 'removed.json'], ['removed.json']);
    const current = result(['kept.json'], []);
    const comparison = compareParityFailures(current, baseline);

    expect(comparison.missingSpecs).toEqual(['removed.json']);
    expect(comparison.retiredSpecs).toEqual([]);
    expect(comparison.resolvedSpecs).toEqual([]);
  });

  it('allows only baseline specs named by the explicit retirement inventory', () => {
    const baseline = result(['kept.json', 'retired.json', 'missing.json'], ['retired.json', 'missing.json']);
    const current = result(['kept.json'], []);
    const comparison = compareParityFailures(current, baseline, new Set(['retired.json']));

    expect(comparison.retiredSpecs).toEqual(['retired.json']);
    expect(comparison.missingSpecs).toEqual(['missing.json']);
    expect(comparison.resolvedSpecs).toEqual([]);
  });

  it('rejects incomplete or contradictory result documents', () => {
    expect(() =>
      parseParityResult({
        schemaVersion: 1,
        selectedSpecs: ['a.json', 'b.json'],
        passedSpecs: ['a.json'],
        failedSpecs: [],
      }),
    ).toThrow(/every selected spec/u);

    expect(() =>
      parseParityResult({
        schemaVersion: 1,
        selectedSpecs: ['a.json'],
        passedSpecs: ['a.json'],
        failedSpecs: [{ specPath: 'a.json', errors: ['mismatch'] }],
      }),
    ).toThrow(/both passed and failed/u);
  });
});
