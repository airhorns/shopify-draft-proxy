/* oxlint-disable no-console -- CLI runner intentionally writes status and error output to stdio. */
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

import { loadConformanceScenarios } from './conformance-scenario-registry.js';

type GleamResult<T, E> = {
  0: T | E;
  isOk(): boolean;
};

type RunnerModule = {
  run(specPath: string): GleamResult<unknown, unknown>;
  into_assert(report: unknown): GleamResult<undefined, string>;
  render(report: unknown): string;
  render_error(error: unknown): string;
};

const scenarioIds = process.argv.slice(2).filter((arg) => arg.length > 0);

if (scenarioIds.length === 0) {
  console.error('Usage: corepack pnpm conformance:run <scenario-id> [scenario-id...]');
  process.exit(1);
}

const repoRoot = process.cwd();
const scenarios = loadConformanceScenarios(repoRoot);
const specs = scenarioIds.map((scenarioId) => {
  const scenario = scenarios.find((candidate) => candidate.id === scenarioId);
  if (!scenario) {
    throw new Error(`No conformance parity scenario found for ${scenarioId}.`);
  }
  return scenario.paritySpecPath;
});

const build = spawnSync('corepack', ['pnpm', 'gleam:build:js'], {
  stdio: 'inherit',
});

if (build.status !== 0) {
  process.exit(build.status ?? 1);
}

const runnerPath = path.resolve(repoRoot, 'build/dev/javascript/shopify_draft_proxy/parity/runner.mjs');
if (!existsSync(runnerPath)) {
  throw new Error(`Compiled parity runner not found at ${runnerPath}.`);
}

const runner = (await import(pathToFileURL(runnerPath).href)) as RunnerModule;
let failed = false;

for (const specPath of specs) {
  const result = runner.run(specPath);
  if (!result.isOk()) {
    console.error(`${specPath}: ${runner.render_error(result[0])}`);
    failed = true;
    continue;
  }

  const report = result[0];
  const assertion = runner.into_assert(report);
  if (!assertion.isOk()) {
    console.error(assertion[0]);
    failed = true;
  } else {
    console.log(runner.render(report));
  }
}

process.exit(failed ? 1 : 0);
