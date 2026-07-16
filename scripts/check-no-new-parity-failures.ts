import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

import { retiredConformanceEvidencePaths } from './conformance-capture-index.js';

export interface ParityFailure {
  specPath: string;
  errors: string[];
}

export interface ParityResult {
  schemaVersion: 1;
  selectedSpecs: string[];
  passedSpecs: string[];
  failedSpecs: ParityFailure[];
}

export interface ParityFailureComparison {
  baselineFailures: string[];
  currentFailures: string[];
  baselineFailureTargets: string[];
  currentFailureTargets: string[];
  missingSpecs: string[];
  retiredSpecs: string[];
  newlyFailingSpecs: string[];
  newlyFailingTargets: string[];
  resolvedSpecs: string[];
  resolvedTargets: string[];
}

function assertStringArray(value: unknown, field: string, unique = true): asserts value is string[] {
  if (!Array.isArray(value) || value.some((entry) => typeof entry !== 'string')) {
    throw new Error(`${field} must be an array of strings`);
  }
  if (unique && new Set(value).size !== value.length) {
    throw new Error(`${field} must not contain duplicates`);
  }
}

export function parseParityResult(value: unknown, source = 'parity result'): ParityResult {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${source} must be a JSON object`);
  }

  const result = value as Record<string, unknown>;
  if (result['schemaVersion'] !== 1) {
    throw new Error(`${source}.schemaVersion must be 1`);
  }

  assertStringArray(result['selectedSpecs'], `${source}.selectedSpecs`);
  assertStringArray(result['passedSpecs'], `${source}.passedSpecs`);
  if (!Array.isArray(result['failedSpecs'])) {
    throw new Error(`${source}.failedSpecs must be an array`);
  }

  const failedSpecs = result['failedSpecs'].map((failure, index): ParityFailure => {
    if (!failure || typeof failure !== 'object' || Array.isArray(failure)) {
      throw new Error(`${source}.failedSpecs[${index}] must be an object`);
    }
    const candidate = failure as Record<string, unknown>;
    if (typeof candidate['specPath'] !== 'string') {
      throw new Error(`${source}.failedSpecs[${index}].specPath must be a string`);
    }
    assertStringArray(candidate['errors'], `${source}.failedSpecs[${index}].errors`, false);
    return { specPath: candidate['specPath'], errors: candidate['errors'] };
  });

  const selected = new Set(result['selectedSpecs']);
  const passed = new Set(result['passedSpecs']);
  const failedPaths = failedSpecs.map(({ specPath }) => specPath);
  if (new Set(failedPaths).size !== failedPaths.length) {
    throw new Error(`${source}.failedSpecs must not contain duplicate spec paths`);
  }
  for (const specPath of [...passed, ...failedPaths]) {
    if (!selected.has(specPath)) {
      throw new Error(`${source} reports unselected spec ${specPath}`);
    }
  }
  for (const specPath of failedPaths) {
    if (passed.has(specPath)) {
      throw new Error(`${source} reports ${specPath} as both passed and failed`);
    }
  }
  if (passed.size + failedPaths.length !== selected.size) {
    throw new Error(`${source} must report every selected spec as passed or failed`);
  }

  return {
    schemaVersion: 1,
    selectedSpecs: result['selectedSpecs'],
    passedSpecs: result['passedSpecs'],
    failedSpecs,
  };
}

export function compareParityFailures(
  current: ParityResult,
  baseline: ParityResult,
  allowedMissingSpecs: ReadonlySet<string> = new Set(),
): ParityFailureComparison {
  const baselineFailures = new Set(baseline.failedSpecs.map(({ specPath }) => specPath));
  const currentFailures = new Set(current.failedSpecs.map(({ specPath }) => specPath));
  const sorted = (values: Iterable<string>): string[] => [...values].sort((left, right) => left.localeCompare(right));
  const failureTargets = (result: ParityResult): Set<string> =>
    new Set(
      result.failedSpecs.flatMap(({ specPath, errors }) =>
        errors.map((error) => {
          const prefix = `${specPath} [`;
          const targetEnd = error.startsWith(prefix) ? error.indexOf('] ', prefix.length) : -1;
          return targetEnd < 0 ? specPath : error.slice(0, targetEnd + 1);
        }),
      ),
    );
  const baselineFailureTargets = failureTargets(baseline);
  const currentFailureTargets = failureTargets(current);
  const currentSelected = new Set(current.selectedSpecs);
  const absentBaselineSpecs = baseline.selectedSpecs.filter((specPath) => !currentSelected.has(specPath));

  return {
    baselineFailures: sorted(baselineFailures),
    currentFailures: sorted(currentFailures),
    baselineFailureTargets: sorted(baselineFailureTargets),
    currentFailureTargets: sorted(currentFailureTargets),
    missingSpecs: sorted(absentBaselineSpecs.filter((specPath) => !allowedMissingSpecs.has(specPath))),
    retiredSpecs: sorted(absentBaselineSpecs.filter((specPath) => allowedMissingSpecs.has(specPath))),
    newlyFailingSpecs: sorted([...currentFailures].filter((specPath) => !baselineFailures.has(specPath))),
    newlyFailingTargets: sorted([...currentFailureTargets].filter((target) => !baselineFailureTargets.has(target))),
    resolvedSpecs: sorted(
      [...baselineFailures].filter((specPath) => currentSelected.has(specPath) && !currentFailures.has(specPath)),
    ),
    resolvedTargets: sorted(
      [...baselineFailureTargets].filter(
        (target) =>
          current.selectedSpecs.some((specPath) => target === specPath || target.startsWith(`${specPath} [`)) &&
          !currentFailureTargets.has(target),
      ),
    ),
  };
}

async function readParityResult(filePath: string): Promise<ParityResult> {
  const source = await readFile(filePath, 'utf8');
  return parseParityResult(JSON.parse(source) as unknown, filePath);
}

function parseArgs(argv: string[]): Map<string, string> {
  const args = new Map<string, string>();
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--') continue;
    if (!arg?.startsWith('--')) throw new Error(`Unexpected positional argument: ${arg}`);
    const value = argv[index + 1];
    if (!value || value.startsWith('--')) throw new Error(`${arg} requires a path argument`);
    args.set(arg.slice(2), value);
    index += 1;
  }
  return args;
}

function writeLine(message: string): void {
  process.stdout.write(`${message}\n`);
}

const invokedPath = process.argv[1];

if (invokedPath && import.meta.url === pathToFileURL(invokedPath).href) {
  const args = parseArgs(process.argv.slice(2));
  const currentPath = args.get('current');
  const baselinePath = args.get('baseline');
  if (!currentPath || !baselinePath) {
    throw new Error('Usage: pnpm parity:check-regressions -- --current <path> --baseline <path>');
  }

  const current = await readParityResult(currentPath);
  const baseline = await readParityResult(baselinePath);
  const comparison = compareParityFailures(current, baseline, new Set(retiredConformanceEvidencePaths));

  writeLine(
    `[parity-regression] current: ${comparison.currentFailures.length}/${current.selectedSpecs.length} failed; main: ${comparison.baselineFailures.length}/${baseline.selectedSpecs.length} failed`,
  );
  if (comparison.resolvedSpecs.length > 0) {
    writeLine(`[parity-regression] resolved since main: ${comparison.resolvedSpecs.join(', ')}`);
  }
  if (comparison.retiredSpecs.length > 0) {
    writeLine(`[parity-regression] explicitly retired since main: ${comparison.retiredSpecs.join(', ')}`);
  }
  if (comparison.missingSpecs.length > 0) {
    writeLine('[parity-regression] specs present on main but absent from the current result:');
    for (const specPath of comparison.missingSpecs) writeLine(`- ${specPath}`);
  }
  if (comparison.newlyFailingSpecs.length > 0) {
    writeLine('[parity-regression] newly failing specs:');
    for (const specPath of comparison.newlyFailingSpecs) {
      writeLine(`- ${specPath}`);
      const failure = current.failedSpecs.find((candidate) => candidate.specPath === specPath);
      for (const error of failure?.errors ?? []) writeLine(`  ${error}`);
    }
  }
  const newlyFailingTargetsInKnownSpecs = comparison.newlyFailingTargets.filter(
    (target) =>
      !comparison.newlyFailingSpecs.some((specPath) => target === specPath || target.startsWith(`${specPath} [`)),
  );
  if (newlyFailingTargetsInKnownSpecs.length > 0) {
    writeLine('[parity-regression] newly failing targets in known-failing specs:');
    for (const target of newlyFailingTargetsInKnownSpecs) writeLine(`- ${target}`);
  }
  if (
    comparison.missingSpecs.length > 0 ||
    comparison.newlyFailingSpecs.length > 0 ||
    newlyFailingTargetsInKnownSpecs.length > 0
  ) {
    process.exitCode = 1;
  } else {
    writeLine('[parity-regression] no new parity failures');
  }
}
