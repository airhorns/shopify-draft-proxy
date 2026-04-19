import { execSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const defaultStatusPath = path.join(repoRoot, 'docs', 'generated', 'conformance-status.json');
const defaultOutputJsonPath = path.join(repoRoot, '.conformance', 'current', 'conformance-status-report.json');
const defaultOutputMarkdownPath = path.join(repoRoot, '.conformance', 'current', 'conformance-status-comment.md');
const commentMarker = '<!-- shopify-draft-proxy-conformance-status -->';

function parseArgs(argv) {
  const args = new Map();

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--') {
      continue;
    }

    if (!arg.startsWith('--')) {
      throw new Error(`Unexpected positional argument: ${arg}`);
    }

    const key = arg.slice(2);
    const next = argv[index + 1];
    if (!next || next.startsWith('--')) {
      args.set(key, 'true');
      continue;
    }

    args.set(key, next);
    index += 1;
  }

  return args;
}

function readJsonFile(filePath) {
  return JSON.parse(readFileSync(filePath, 'utf8'));
}

function ratio(numerator, denominator) {
  return denominator === 0 ? 0 : numerator / denominator;
}

function formatPercent(value) {
  return `${(value * 100).toFixed(1)}%`;
}

function formatSignedInteger(value) {
  return value > 0 ? `+${value}` : String(value);
}

function formatSignedPercentPoints(value) {
  const formatted = `${Math.abs(value).toFixed(1)} percentage points`;
  return value > 0 ? `+${formatted}` : value < 0 ? `-${formatted}` : formatted;
}

function getGitValue(args, envName, command) {
  const fromEnv = process.env[envName];
  if (fromEnv && fromEnv.length > 0) {
    return fromEnv;
  }

  try {
    return execSync(command, { cwd: repoRoot, encoding: 'utf8' }).trim();
  } catch {
    return null;
  }
}

export function summarizeConformanceStatus(status) {
  const conformingScenarioIds = Array.isArray(status.capturedScenarioIds) ? status.capturedScenarioIds : [];
  const pendingScenarioIds = Array.isArray(status.plannedScenarioIds) ? status.plannedScenarioIds : [];
  const coveredOperationNames = Array.isArray(status.coveredOperationNames) ? status.coveredOperationNames : [];
  const declaredGapOperationNames = Array.isArray(status.declaredGapOperationNames) ? status.declaredGapOperationNames : [];
  const implementedOperations = Array.isArray(status.implementedOperations) ? status.implementedOperations : [];
  const totalScenarios = conformingScenarioIds.length + pendingScenarioIds.length;

  return {
    generatedAt: status.generatedAt ?? null,
    conformingScenarios: conformingScenarioIds.length,
    totalScenarios,
    pendingScenarios: pendingScenarioIds.length,
    conformanceRatio: ratio(conformingScenarioIds.length, totalScenarios),
    coveredOperations: coveredOperationNames.length,
    implementedOperations: implementedOperations.length,
    declaredGapOperations: declaredGapOperationNames.length,
    operationCoverageRatio: ratio(coveredOperationNames.length, implementedOperations.length),
  };
}

function normalizeBaseline(rawBaseline) {
  if (typeof rawBaseline?.conformingScenarios === 'number') {
    return rawBaseline;
  }

  if (rawBaseline?.conformance && typeof rawBaseline.conformance.conformingScenarios === 'number') {
    return rawBaseline.conformance;
  }

  return summarizeConformanceStatus(rawBaseline);
}

export function compareConformanceSummaries(current, baseline) {
  if (!baseline) {
    return null;
  }

  return {
    conformingScenarios: current.conformingScenarios - baseline.conformingScenarios,
    totalScenarios: current.totalScenarios - baseline.totalScenarios,
    conformanceRatio: current.conformanceRatio - baseline.conformanceRatio,
    coveredOperations: current.coveredOperations - baseline.coveredOperations,
    implementedOperations: current.implementedOperations - baseline.implementedOperations,
    declaredGapOperations: current.declaredGapOperations - baseline.declaredGapOperations,
  };
}

export function renderConformanceComment(report) {
  const baseline = report.baseline;
  const delta = report.delta;
  const current = report.conformance;
  const baselineLine = baseline
    ? `- Main baseline: ${baseline.conformingScenarios} / ${baseline.totalScenarios} scenarios (${formatPercent(baseline.conformanceRatio)})`
    : '- Main baseline: not found yet. The next successful push to `main` will publish one.';
  const improvementLine = delta
    ? `- Improvement over main: ${formatSignedInteger(delta.conformingScenarios)} conforming scenarios (${formatSignedPercentPoints(delta.conformanceRatio * 100)})`
    : '- Improvement over main: unavailable until a main baseline artifact exists.';
  const commit = report.commit ? report.commit.slice(0, 12) : 'unknown';

  return [
    commentMarker,
    '## Conformance status',
    '',
    `- Current branch: ${current.conformingScenarios} / ${current.totalScenarios} scenarios conforming (${formatPercent(current.conformanceRatio)})`,
    baselineLine,
    improvementLine,
    `- Covered operations: ${current.coveredOperations} / ${current.implementedOperations} (${current.declaredGapOperations} declared gaps)`,
    `- Commit: \`${commit}\``,
    '',
    '_Generated from `corepack pnpm conformance:check`._',
    '',
  ].join('\n');
}

export function buildConformanceReport({ status, baseline = null, commit = null, refName = null, runId = null }) {
  const conformance = summarizeConformanceStatus(status);
  const normalizedBaseline = baseline ? normalizeBaseline(baseline) : null;

  return {
    generatedAt: new Date().toISOString(),
    sourceStatusGeneratedAt: status.generatedAt ?? null,
    commit,
    refName,
    runId,
    conformance,
    baseline: normalizedBaseline,
    delta: compareConformanceSummaries(conformance, normalizedBaseline),
  };
}

export function writeConformanceReport({ statusPath, baselinePath, outputJsonPath, outputMarkdownPath }) {
  const status = readJsonFile(statusPath);
  const baseline = baselinePath && existsSync(baselinePath) ? readJsonFile(baselinePath) : null;
  const report = buildConformanceReport({
    status,
    baseline,
    commit: getGitValue(null, 'GITHUB_SHA', 'git rev-parse HEAD'),
    refName: getGitValue(null, 'GITHUB_REF_NAME', 'git branch --show-current'),
    runId: process.env.GITHUB_RUN_ID ?? null,
  });
  const markdown = renderConformanceComment(report);

  mkdirSync(path.dirname(outputJsonPath), { recursive: true });
  mkdirSync(path.dirname(outputMarkdownPath), { recursive: true });
  writeFileSync(outputJsonPath, JSON.stringify(report, null, 2) + '\n');
  writeFileSync(outputMarkdownPath, markdown);

  console.log(
    `conformance status: ${report.conformance.conformingScenarios}/${report.conformance.totalScenarios} scenarios conforming`,
  );
  if (report.delta) {
    console.log(`improvement over main: ${formatSignedInteger(report.delta.conformingScenarios)} scenarios`);
  } else {
    console.log('improvement over main: baseline unavailable');
  }
}

function printHelp() {
  console.log(`Usage: node scripts/conformance-status-report.mjs [options]

Options:
  --status-json <path>       Source conformance status JSON. Defaults to docs/generated/conformance-status.json.
  --baseline <path>          Optional main baseline report JSON.
  --output-json <path>       Output report JSON. Defaults to .conformance/current/conformance-status-report.json.
  --output-markdown <path>   Output PR comment markdown. Defaults to .conformance/current/conformance-status-comment.md.
  --help                     Show this help.
`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = parseArgs(process.argv.slice(2));
  if (args.has('help')) {
    printHelp();
  } else {
    writeConformanceReport({
      statusPath: path.resolve(repoRoot, args.get('status-json') ?? defaultStatusPath),
      baselinePath: args.has('baseline') ? path.resolve(repoRoot, args.get('baseline')) : null,
      outputJsonPath: path.resolve(repoRoot, args.get('output-json') ?? defaultOutputJsonPath),
      outputMarkdownPath: path.resolve(repoRoot, args.get('output-markdown') ?? defaultOutputMarkdownPath),
    });
  }
}
