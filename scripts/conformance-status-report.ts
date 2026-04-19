import { execSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const defaultStatusPath = path.join(repoRoot, 'docs', 'generated', 'conformance-status.json');
const defaultOutputJsonPath = path.join(repoRoot, '.conformance', 'current', 'conformance-status-report.json');
const defaultOutputMarkdownPath = path.join(repoRoot, '.conformance', 'current', 'conformance-status-comment.md');
const commentMarker = '<!-- shopify-draft-proxy-conformance-status -->';

export interface ConformanceStatusDocument {
  generatedAt?: string | null;
  capturedScenarioIds?: unknown[] | null;
  plannedScenarioIds?: unknown[] | null;
  coveredOperationNames?: unknown[] | null;
  declaredGapOperationNames?: unknown[] | null;
  implementedOperations?: unknown[] | null;
}

export interface ConformanceSummary {
  generatedAt: string | null;
  conformingScenarios: number;
  totalScenarios: number;
  pendingScenarios: number;
  conformanceRatio: number;
  coveredOperations: number;
  implementedOperations: number;
  declaredGapOperations: number;
  operationCoverageRatio: number;
}

export interface ConformanceDelta {
  conformingScenarios: number;
  totalScenarios: number;
  conformanceRatio: number;
  coveredOperations: number;
  implementedOperations: number;
  declaredGapOperations: number;
}

export interface ConformanceReport {
  generatedAt: string;
  sourceStatusGeneratedAt: string | null;
  commit: string | null;
  refName: string | null;
  runId: string | null;
  conformance: ConformanceSummary;
  baseline: ConformanceSummary | null;
  delta: ConformanceDelta | null;
}

interface ConformanceReportInput {
  status: ConformanceStatusDocument;
  baseline?: ConformanceStatusDocument | ConformanceSummary | ConformanceReport | null;
  commit?: string | null;
  refName?: string | null;
  runId?: string | null;
}

interface WriteConformanceReportInput {
  statusPath: string;
  baselinePath: string | null;
  outputJsonPath: string;
  outputMarkdownPath: string;
}

function parseArgs(argv: string[]): Map<string, string> {
  const args = new Map<string, string>();

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--') {
      continue;
    }

    if (!arg?.startsWith('--')) {
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function readJsonFile(filePath: string): unknown {
  return JSON.parse(readFileSync(filePath, 'utf8')) as unknown;
}

function readConformanceStatus(filePath: string): ConformanceStatusDocument {
  const value = readJsonFile(filePath);
  if (!isRecord(value)) {
    throw new Error(`Expected ${filePath} to contain a JSON object.`);
  }

  return value;
}

function readBaseline(filePath: string): ConformanceStatusDocument | ConformanceSummary | ConformanceReport {
  const value = readJsonFile(filePath);
  if (!isRecord(value)) {
    throw new Error(`Expected ${filePath} to contain a JSON object.`);
  }

  return value as ConformanceStatusDocument | ConformanceSummary | ConformanceReport;
}

function ratio(numerator: number, denominator: number): number {
  return denominator === 0 ? 0 : numerator / denominator;
}

function formatPercent(value: number): string {
  return `${(value * 100).toFixed(1)}%`;
}

function formatSignedInteger(value: number): string {
  return value > 0 ? `+${value}` : String(value);
}

function formatSignedPercentPoints(value: number): string {
  const formatted = `${Math.abs(value).toFixed(1)} percentage points`;
  return value > 0 ? `+${formatted}` : value < 0 ? `-${formatted}` : formatted;
}

function writeLine(message: string): void {
  process.stdout.write(`${message}\n`);
}

function getGitValue(envName: string, command: string): string | null {
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

export function summarizeConformanceStatus(status: ConformanceStatusDocument): ConformanceSummary {
  const conformingScenarioIds = Array.isArray(status.capturedScenarioIds) ? status.capturedScenarioIds : [];
  const pendingScenarioIds = Array.isArray(status.plannedScenarioIds) ? status.plannedScenarioIds : [];
  const coveredOperationNames = Array.isArray(status.coveredOperationNames) ? status.coveredOperationNames : [];
  const declaredGapOperationNames = Array.isArray(status.declaredGapOperationNames)
    ? status.declaredGapOperationNames
    : [];
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

function isConformanceSummary(value: unknown): value is ConformanceSummary {
  return isRecord(value) && typeof value['conformingScenarios'] === 'number';
}

function isConformanceReport(value: unknown): value is ConformanceReport {
  return isRecord(value) && isConformanceSummary(value['conformance']);
}

function normalizeBaseline(
  rawBaseline: ConformanceStatusDocument | ConformanceSummary | ConformanceReport,
): ConformanceSummary {
  if (isConformanceSummary(rawBaseline)) {
    return rawBaseline;
  }

  if (isConformanceReport(rawBaseline)) {
    return rawBaseline.conformance;
  }

  return summarizeConformanceStatus(rawBaseline);
}

export function compareConformanceSummaries(
  current: ConformanceSummary,
  baseline: ConformanceSummary | null,
): ConformanceDelta | null {
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

export function renderConformanceComment(report: ConformanceReport): string {
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

export function buildConformanceReport({
  status,
  baseline = null,
  commit = null,
  refName = null,
  runId = null,
}: ConformanceReportInput): ConformanceReport {
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

export function writeConformanceReport({
  statusPath,
  baselinePath,
  outputJsonPath,
  outputMarkdownPath,
}: WriteConformanceReportInput): void {
  const status = readConformanceStatus(statusPath);
  const baseline = baselinePath && existsSync(baselinePath) ? readBaseline(baselinePath) : null;
  const report = buildConformanceReport({
    status,
    baseline,
    commit: getGitValue('GITHUB_SHA', 'git rev-parse HEAD'),
    refName: getGitValue('GITHUB_REF_NAME', 'git branch --show-current'),
    runId: process.env['GITHUB_RUN_ID'] ?? null,
  });
  const markdown = renderConformanceComment(report);

  mkdirSync(path.dirname(outputJsonPath), { recursive: true });
  mkdirSync(path.dirname(outputMarkdownPath), { recursive: true });
  writeFileSync(outputJsonPath, JSON.stringify(report, null, 2) + '\n');
  writeFileSync(outputMarkdownPath, markdown);

  writeLine(
    `conformance status: ${report.conformance.conformingScenarios}/${report.conformance.totalScenarios} scenarios conforming`,
  );
  if (report.delta) {
    writeLine(`improvement over main: ${formatSignedInteger(report.delta.conformingScenarios)} scenarios`);
  } else {
    writeLine('improvement over main: baseline unavailable');
  }
}

function printHelp(): void {
  process.stdout.write(`Usage: tsx scripts/conformance-status-report.ts [options]

Options:
  --status-json <path>       Source conformance status JSON. Defaults to docs/generated/conformance-status.json.
  --baseline <path>          Optional main baseline report JSON.
  --output-json <path>       Output report JSON. Defaults to .conformance/current/conformance-status-report.json.
  --output-markdown <path>   Output PR comment markdown. Defaults to .conformance/current/conformance-status-comment.md.
  --help                     Show this help.
`);
}

const invokedPath = process.argv[1];

if (invokedPath && import.meta.url === pathToFileURL(invokedPath).href) {
  const args = parseArgs(process.argv.slice(2));
  if (args.has('help')) {
    printHelp();
  } else {
    writeConformanceReport({
      statusPath: path.resolve(repoRoot, args.get('status-json') ?? defaultStatusPath),
      baselinePath: args.has('baseline') ? path.resolve(repoRoot, args.get('baseline') ?? '') : null,
      outputJsonPath: path.resolve(repoRoot, args.get('output-json') ?? defaultOutputJsonPath),
      outputMarkdownPath: path.resolve(repoRoot, args.get('output-markdown') ?? defaultOutputMarkdownPath),
    });
  }
}
