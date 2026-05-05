// Pure spec-validation helpers for parity scenarios. The seed-based
// runner that used to live alongside these functions has been retired
// (cassette playback now drives parity from `test/parity/...`).
// Only the spec-shape validators remain here, used by:
//   - scripts/gleam-port-coverage-gate.ts
//   - tests/unit/conformance-scenario-discovery.test.ts

import type { Matcher, ParitySpec } from './support/json-schemas.js';

export type { ExpectedDifference, Matcher, ParitySpec, ProxyRequestSpec } from './support/json-schemas.js';

export type ParityScenarioState =
  | 'ready-for-comparison'
  | 'enforced-by-fixture'
  | 'invalid-missing-comparison-contract'
  | 'not-yet-implemented';

export interface Scenario {
  id: string;
  status: string;
  operationNames?: string[];
  assertionKinds?: string[];
  captureFiles?: string[];
  paritySpecPath?: string;
}

export const parityStatusNote =
  'readyForComparison means a captured scenario has a proxy request and an explicit strict-json comparison contract. enforcedByFixture means a captured multi-step fixture is enforced outside the generic parity runner by committed runtime tests. invalid captured scenarios are not allowed in checked-in inventory. notYetImplemented scenarios are legacy non-executable entries; do not add new planned-only parity specs.';

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasProxyRequest(paritySpec: ParitySpec | null | undefined): boolean {
  return !!paritySpec?.proxyRequest?.documentPath;
}

function hasComparisonContract(paritySpec: ParitySpec | null | undefined): boolean {
  if (validateComparisonContract(paritySpec?.comparison).length > 0) {
    return false;
  }
  const targets = paritySpec?.comparison?.targets;
  return Array.isArray(targets) && targets.length > 0;
}

function isKnownMatcher(matcher: string): matcher is Matcher {
  return (
    matcher === 'any-string' ||
    matcher === 'non-empty-string' ||
    matcher === 'any-number' ||
    matcher === 'iso-timestamp' ||
    /^shopify-gid:([A-Za-z][A-Za-z0-9]*)$/.test(matcher) ||
    /^shop-policy-url-base:https:\/\/[^/\s]+(?:\/[^\s]*)?$/.test(matcher) ||
    /^exact-string:.+$/.test(matcher) ||
    /^regex:\^.+$/.test(matcher)
  );
}

function validateExpectedDifferences(rawRules: unknown, labelPrefix: string): string[] {
  const errors: string[] = [];
  if (!Array.isArray(rawRules)) {
    errors.push(`${labelPrefix} must declare an expectedDifferences array.`);
    return errors;
  }

  for (const [index, rawRule] of rawRules.entries()) {
    const rule = isPlainObject(rawRule) ? rawRule : {};
    const label = `${labelPrefix}[${index}]`;
    if (typeof rule['path'] !== 'string' || rule['path'].length === 0) {
      errors.push(`${label} must declare a non-empty JSON path.`);
    }

    if (typeof rule['reason'] !== 'string' || rule['reason'].length === 0) {
      errors.push(`${label} must document why the expected difference is accepted.`);
    }

    const hasMatcher = typeof rule['matcher'] === 'string';
    const isIgnored = rule['ignore'] === true;
    if (hasMatcher === isIgnored) {
      errors.push(`${label} must declare exactly one of \`matcher\` or \`ignore: true\`.`);
    }

    if (hasMatcher && !isKnownMatcher(rule['matcher'] as string)) {
      errors.push(`${label} declares unknown matcher \`${String(rule['matcher'])}\`.`);
    }

    if ('regrettable' in rule && rule['regrettable'] !== true) {
      errors.push(`${label} \`regrettable\`, when declared, must be true.`);
    }

    if (isIgnored && rule['regrettable'] !== true) {
      errors.push(`${label} with \`ignore: true\` must set \`regrettable: true\` for the parity gap.`);
    }
  }

  return errors;
}

export function validateComparisonContract(comparison: unknown): string[] {
  const errors: string[] = [];
  const candidate = isPlainObject(comparison) ? comparison : {};

  if (candidate['mode'] !== 'strict-json') {
    errors.push('Comparison contract mode must be `strict-json`.');
  }

  if ('allowedDifferences' in candidate) {
    errors.push('Comparison contract must use `expectedDifferences`; `allowedDifferences` is no longer supported.');
  }

  if (!Array.isArray(candidate['expectedDifferences'])) {
    errors.push('Comparison contract must declare an `expectedDifferences` array.');
    return errors;
  }

  errors.push(...validateExpectedDifferences(candidate['expectedDifferences'], 'expectedDifferences'));

  const rawTargets = candidate['targets'];
  if (rawTargets !== undefined) {
    if (!Array.isArray(rawTargets) || rawTargets.length === 0) {
      errors.push('Comparison contract `targets`, when declared, must be a non-empty array.');
    } else {
      for (const [index, rawTarget] of rawTargets.entries()) {
        const target = isPlainObject(rawTarget) ? rawTarget : {};
        const label = `targets[${index}]`;
        if (typeof target['name'] !== 'string' || target['name'].length === 0) {
          errors.push(`${label} must declare a non-empty name.`);
        }
        if (typeof target['capturePath'] !== 'string' || target['capturePath'].length === 0) {
          errors.push(`${label} must declare a non-empty capturePath.`);
        }
        const outputPaths = ['proxyPath', 'proxyStatePath', 'proxyLogPath'].filter(
          (pathKey) => typeof target[pathKey] === 'string' && (target[pathKey] as string).length > 0,
        );
        if (outputPaths.length !== 1) {
          errors.push(`${label} must declare exactly one non-empty proxyPath, proxyStatePath, or proxyLogPath.`);
        }
        if ('selectedPaths' in target) {
          if (!Array.isArray(target['selectedPaths']) || target['selectedPaths'].length === 0) {
            errors.push(`${label} selectedPaths, when declared, must be a non-empty array.`);
          } else {
            for (const [pathIndex, rawPath] of target['selectedPaths'].entries()) {
              if (typeof rawPath !== 'string' || rawPath.length === 0) {
                errors.push(`${label}.selectedPaths[${pathIndex}] must be a non-empty JSON path.`);
              }
            }
          }
        }
        if ('excludedPaths' in target) {
          if (!Array.isArray(target['excludedPaths']) || target['excludedPaths'].length === 0) {
            errors.push(`${label} excludedPaths, when declared, must be a non-empty array.`);
          } else {
            for (const [pathIndex, rawPath] of target['excludedPaths'].entries()) {
              if (typeof rawPath !== 'string' || rawPath.length === 0) {
                errors.push(`${label}.excludedPaths[${pathIndex}] must be a non-empty JSON path.`);
              }
            }
          }
        }
        if ('selectedPaths' in target && 'excludedPaths' in target) {
          errors.push(`${label} must not declare both selectedPaths and excludedPaths.`);
        }
        if ('expectedDifferences' in target) {
          errors.push(...validateExpectedDifferences(target['expectedDifferences'], `${label}.expectedDifferences`));
        }
      }
    }
  }

  return errors;
}

export function classifyParityScenarioState(
  scenario: Pick<Scenario, 'status'>,
  paritySpec: ParitySpec | null | undefined,
): ParityScenarioState {
  if (scenario.status === 'captured') {
    if (paritySpec?.comparisonMode === 'captured-fixture' && (paritySpec.liveCaptureFiles?.length ?? 0) > 0) {
      return 'enforced-by-fixture';
    }

    return hasProxyRequest(paritySpec) && hasComparisonContract(paritySpec)
      ? 'ready-for-comparison'
      : 'invalid-missing-comparison-contract';
  }

  return 'not-yet-implemented';
}

export function validateParityScenarioInventoryEntry(
  scenario: Pick<Scenario, 'id' | 'status' | 'captureFiles'>,
  paritySpec: ParitySpec,
): string[] {
  const errors: string[] = [];
  const mode = paritySpec.comparisonMode;

  if (scenario.status !== 'captured') {
    return errors;
  }

  if (mode === 'planned') {
    errors.push(`Captured scenario ${scenario.id} must use an enforced captured comparison mode.`);
    return errors;
  }

  if (mode === 'captured-fixture') {
    if ((scenario.captureFiles?.length ?? paritySpec.liveCaptureFiles?.length ?? 0) === 0) {
      errors.push(`Captured fixture scenario ${scenario.id} must reference at least one capture fixture.`);
    }
    if ((paritySpec.runtimeTestFiles?.length ?? 0) === 0) {
      errors.push(`Captured fixture scenario ${scenario.id} must reference at least one runtime test file.`);
    }
    return errors;
  }

  if (!hasProxyRequest(paritySpec)) {
    errors.push(`Captured scenario ${scenario.id} must declare a proxy request.`);
  }

  const comparisonErrors = validateComparisonContract(paritySpec.comparison);
  if (comparisonErrors.length > 0) {
    errors.push(...comparisonErrors.map((error) => `Captured scenario ${scenario.id}: ${error}`));
  }

  if (!hasComparisonContract(paritySpec)) {
    errors.push(`Captured scenario ${scenario.id} must declare at least one executable comparison target.`);
  }

  return errors;
}
