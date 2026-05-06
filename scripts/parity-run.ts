import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

type CliArgs = {
  all: boolean;
  debug: boolean;
  scenarioIds: string[];
  specPaths: string[];
};

type ParitySpecHeader = {
  scenarioId?: string;
};

type GleamResult<T> = { constructor: { name: 'Ok' }; 0: T } | { constructor: { name: 'Error' }; 0: unknown };

type ParityRunner = {
  run: (specPath: string) => GleamResult<unknown>;
  run_debug: (specPath: string) => GleamResult<unknown>;
  into_assert: (report: unknown) => GleamResult<void>;
  render: (report: unknown) => string;
  render_error: (error: unknown) => string;
};

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const paritySpecRoot = path.join(repoRoot, 'config', 'parity-specs');
const gleamRunnerPath = path.join(
  repoRoot,
  'build',
  'dev',
  'javascript',
  'shopify_draft_proxy',
  'parity',
  'runner.mjs',
);

function log(message: string): void {
  process.stdout.write(`${message}\n`);
}

function logError(message: string): void {
  process.stderr.write(`${message}\n`);
}

function parseArgs(argv: string[]): CliArgs {
  const args: CliArgs = { all: false, debug: false, scenarioIds: [], specPaths: [] };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index] ?? '';
    if (arg === '--') {
      continue;
    } else if (arg === '--all') {
      args.all = true;
    } else if (arg === '--debug') {
      args.debug = true;
    } else if (arg === '--spec') {
      const next = argv[index + 1];
      if (!next || next.startsWith('-')) {
        throw new Error('--spec requires a path argument');
      }
      args.specPaths.push(next);
      index += 1;
    } else if (arg.startsWith('-')) {
      throw new Error(`Unknown flag: ${arg}`);
    } else {
      args.scenarioIds.push(arg);
    }
  }

  return args;
}

async function findAllSpecPaths(directory = paritySpecRoot): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async (entry) => {
      const fullPath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        return findAllSpecPaths(fullPath);
      }

      return entry.isFile() && entry.name.endsWith('.json') ? [fullPath] : [];
    }),
  );

  return nested.flat().sort();
}

async function findSpecForScenario(scenarioId: string): Promise<string> {
  const specPaths = await findAllSpecPaths();
  for (const specPath of specPaths) {
    try {
      const parsed = JSON.parse(await readFile(specPath, 'utf8')) as ParitySpecHeader;
      if (parsed.scenarioId === scenarioId) {
        return specPath;
      }
    } catch {
      // Invalid spec JSON is reported by conformance checks and the Gleam runner.
    }
  }

  throw new Error(`No parity spec with scenarioId "${scenarioId}" found under config/parity-specs/`);
}

async function resolveSpecPaths(args: CliArgs): Promise<string[]> {
  if (args.all) {
    return findAllSpecPaths();
  }

  const specPaths: string[] = [];
  for (const scenarioId of args.scenarioIds) {
    specPaths.push(await findSpecForScenario(scenarioId));
  }
  for (const specPath of args.specPaths) {
    specPaths.push(path.isAbsolute(specPath) ? specPath : path.resolve(repoRoot, specPath));
  }

  return specPaths;
}

function isOk<T>(result: GleamResult<T>): result is { constructor: { name: 'Ok' }; 0: T } {
  return result.constructor.name === 'Ok';
}

async function main(): Promise<void> {
  let args: CliArgs;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (error) {
    logError((error as Error).message);
    logError('Usage: pnpm parity <scenario-id> | --spec <path> | --all [--debug]');
    process.exit(2);
    return;
  }

  if (!args.all && args.scenarioIds.length === 0 && args.specPaths.length === 0) {
    logError('Usage: pnpm parity <scenario-id> | --spec <path> | --all [--debug]');
    process.exit(2);
    return;
  }

  const runner = (await import(pathToFileURL(gleamRunnerPath).href)) as ParityRunner;
  const specPaths = await resolveSpecPaths(args);
  let failures = 0;

  for (const specPath of specPaths) {
    const relativeSpecPath = path.relative(repoRoot, specPath);
    const runResult = args.debug ? runner.run_debug(relativeSpecPath) : runner.run(relativeSpecPath);
    if (!isOk(runResult)) {
      failures += 1;
      logError(`[parity] ${relativeSpecPath}: ${runner.render_error(runResult[0])}`);
      continue;
    }

    const assertResult = runner.into_assert(runResult[0]);
    if (!isOk(assertResult)) {
      failures += 1;
      logError(`[parity] ${relativeSpecPath}: ${String(assertResult[0])}`);
      continue;
    }

    log(`[parity] ${runner.render(runResult[0])}`);
  }

  if (failures > 0) {
    logError(`[parity] ${failures}/${specPaths.length} spec(s) failed`);
    process.exit(1);
  }
}

await main();
