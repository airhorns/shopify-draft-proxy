import { execSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { buildConformanceStatusDocument } from './conformance-scenario-registry.js';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const defaultOutputJsonPath = path.join(repoRoot, '.conformance', 'current', 'conformance-status-report.json');
const defaultOutputMarkdownPath = path.join(repoRoot, '.conformance', 'current', 'conformance-status-comment.md');
const commentMarker = '<!-- shopify-draft-proxy-conformance-status -->';

export interface ConformanceStatusDocument {
  generatedAt?: string | null;
  capturedScenarioIds?: unknown[] | null;
  strictComparisonScenarioIds?: unknown[] | null;
  runtimeFixtureScenarioIds?: unknown[] | null;
  captureOnlyScenarioIds?: unknown[] | null;
  plannedScenarioIds?: unknown[] | null;
  coveredOperationNames?: unknown[] | null;
  declaredGapOperationNames?: unknown[] | null;
  implementedOperations?: unknown[] | null;
  apiSurfaceSummaries?: unknown | null;
  regrettableDivergences?: unknown[] | null;
}

interface ApiSurfaceStatusSummary {
  implementedOperations: number;
  coveredOperations: number;
  declaredGapOperations: number;
  operationCoverageRatio: number;
  coveredOperationNames: string[];
  declaredGapOperationNames: string[];
}

export interface ConformanceSummary {
  generatedAt: string | null;
  conformingScenarios: number;
  strictComparisonScenarios?: number;
  runtimeFixtureScenarios?: number;
  runtimeFixtureScenarioIds?: string[];
  totalScenarios: number;
  captureOnlyScenarios?: number;
  captureOnlyScenarioIds?: string[];
  captureOnlyScenariosKnown?: boolean;
  pendingScenarios: number;
  conformanceRatio: number;
  coveredOperations: number;
  implementedOperations: number;
  declaredGapOperations: number;
  operationCoverageRatio: number;
  apiSurfaceSummaries?: Record<string, ApiSurfaceStatusSummary>;
  regrettableDivergences: number;
  regrettableDivergenceScenarios: number;
}

export interface ConformanceDelta {
  conformingScenarios: number;
  totalScenarios: number;
  runtimeFixtureScenarios: number;
  captureOnlyScenarios: number;
  captureOnlyScenariosKnown: boolean;
  conformanceRatio: number;
  coveredOperations: number;
  implementedOperations: number;
  declaredGapOperations: number;
  regrettableDivergences: number;
  regrettableDivergenceScenarios: number;
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
  statusPath: string | null;
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

function stringList(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

function apiSurfaceSummaries(value: unknown): Record<string, ApiSurfaceStatusSummary> | undefined {
  if (!isRecord(value)) {
    return undefined;
  }

  const result: Record<string, ApiSurfaceStatusSummary> = {};
  for (const [name, summary] of Object.entries(value)) {
    if (!isRecord(summary)) {
      continue;
    }
    const implementedOperations = summary['implementedOperations'];
    const coveredOperations = summary['coveredOperations'];
    const declaredGapOperations = summary['declaredGapOperations'];
    const operationCoverageRatio = summary['operationCoverageRatio'];
    if (
      typeof implementedOperations !== 'number' ||
      typeof coveredOperations !== 'number' ||
      typeof declaredGapOperations !== 'number' ||
      typeof operationCoverageRatio !== 'number'
    ) {
      continue;
    }
    result[name] = {
      implementedOperations,
      coveredOperations,
      declaredGapOperations,
      operationCoverageRatio,
      coveredOperationNames: stringList(summary['coveredOperationNames']),
      declaredGapOperationNames: stringList(summary['declaredGapOperationNames']),
    };
  }
  return Object.keys(result).length > 0 ? result : undefined;
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
  const capturedScenarioIds = stringList(status.capturedScenarioIds);
  const hasStrictComparisonBreakdown = Array.isArray(status.strictComparisonScenarioIds);
  const hasCaptureOnlyBreakdown = Array.isArray(status.captureOnlyScenarioIds);
  const captureOnlyScenarioIds = stringList(status.captureOnlyScenarioIds);
  const captureOnlyScenarioIdSet = new Set(captureOnlyScenarioIds);
  const runtimeFixtureScenarioIds = stringList(status.runtimeFixtureScenarioIds);
  const runtimeFixtureScenarioIdSet = new Set(runtimeFixtureScenarioIds);
  const strictComparisonScenarioIds = hasStrictComparisonBreakdown
    ? stringList(status.strictComparisonScenarioIds)
    : capturedScenarioIds.filter((scenarioId) => {
        return !captureOnlyScenarioIdSet.has(scenarioId) && !runtimeFixtureScenarioIdSet.has(scenarioId);
      });
  const pendingScenarioIds = stringList(status.plannedScenarioIds);
  const coveredOperationNames = stringList(status.coveredOperationNames);
  const declaredGapOperationNames = Array.isArray(status.declaredGapOperationNames)
    ? status.declaredGapOperationNames
    : [];
  const implementedOperations = Array.isArray(status.implementedOperations) ? status.implementedOperations : [];
  const surfaceSummaries = apiSurfaceSummaries(status.apiSurfaceSummaries);
  const aggregateSummary = surfaceSummaries?.['aggregate'];
  const regrettableDivergences = Array.isArray(status.regrettableDivergences) ? status.regrettableDivergences : [];
  const regrettableDivergenceScenarioIds = new Set<string>();
  for (const divergence of regrettableDivergences) {
    if (isRecord(divergence) && typeof divergence['scenarioId'] === 'string') {
      regrettableDivergenceScenarioIds.add(divergence['scenarioId']);
    }
  }
  const conformingScenarios = strictComparisonScenarioIds.length + runtimeFixtureScenarioIds.length;
  const totalScenarios = conformingScenarios + captureOnlyScenarioIds.length + pendingScenarioIds.length;

  return {
    generatedAt: status.generatedAt ?? null,
    conformingScenarios,
    strictComparisonScenarios: strictComparisonScenarioIds.length,
    runtimeFixtureScenarios: runtimeFixtureScenarioIds.length,
    runtimeFixtureScenarioIds,
    totalScenarios,
    captureOnlyScenarios: captureOnlyScenarioIds.length,
    captureOnlyScenarioIds,
    captureOnlyScenariosKnown: hasStrictComparisonBreakdown || hasCaptureOnlyBreakdown,
    pendingScenarios: pendingScenarioIds.length,
    conformanceRatio: ratio(conformingScenarios, totalScenarios),
    coveredOperations: aggregateSummary?.coveredOperations ?? coveredOperationNames.length,
    implementedOperations: aggregateSummary?.implementedOperations ?? implementedOperations.length,
    declaredGapOperations: aggregateSummary?.declaredGapOperations ?? declaredGapOperationNames.length,
    operationCoverageRatio:
      aggregateSummary?.operationCoverageRatio ?? ratio(coveredOperationNames.length, implementedOperations.length),
    ...(surfaceSummaries ? { apiSurfaceSummaries: surfaceSummaries } : {}),
    regrettableDivergences: regrettableDivergences.length,
    regrettableDivergenceScenarios: regrettableDivergenceScenarioIds.size,
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
    const captureOnlyScenariosKnown =
      rawBaseline.captureOnlyScenariosKnown === true || typeof rawBaseline.captureOnlyScenarios === 'number';
    return {
      ...rawBaseline,
      strictComparisonScenarios:
        typeof rawBaseline.strictComparisonScenarios === 'number'
          ? rawBaseline.strictComparisonScenarios
          : rawBaseline.conformingScenarios,
      runtimeFixtureScenarios:
        typeof rawBaseline.runtimeFixtureScenarios === 'number' ? rawBaseline.runtimeFixtureScenarios : 0,
      runtimeFixtureScenarioIds: Array.isArray(rawBaseline.runtimeFixtureScenarioIds)
        ? rawBaseline.runtimeFixtureScenarioIds
        : [],
      captureOnlyScenarios: typeof rawBaseline.captureOnlyScenarios === 'number' ? rawBaseline.captureOnlyScenarios : 0,
      captureOnlyScenarioIds: Array.isArray(rawBaseline.captureOnlyScenarioIds)
        ? rawBaseline.captureOnlyScenarioIds
        : [],
      captureOnlyScenariosKnown,
      regrettableDivergences:
        typeof rawBaseline.regrettableDivergences === 'number' ? rawBaseline.regrettableDivergences : 0,
      regrettableDivergenceScenarios:
        typeof rawBaseline.regrettableDivergenceScenarios === 'number' ? rawBaseline.regrettableDivergenceScenarios : 0,
    };
  }

  if (isConformanceReport(rawBaseline)) {
    return normalizeBaseline(rawBaseline.conformance);
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
    runtimeFixtureScenarios: (current.runtimeFixtureScenarios ?? 0) - (baseline.runtimeFixtureScenarios ?? 0),
    captureOnlyScenarios: (current.captureOnlyScenarios ?? 0) - (baseline.captureOnlyScenarios ?? 0),
    captureOnlyScenariosKnown:
      current.captureOnlyScenariosKnown === true && baseline.captureOnlyScenariosKnown === true,
    conformanceRatio: current.conformanceRatio - baseline.conformanceRatio,
    coveredOperations: current.coveredOperations - baseline.coveredOperations,
    implementedOperations: current.implementedOperations - baseline.implementedOperations,
    declaredGapOperations: current.declaredGapOperations - baseline.declaredGapOperations,
    regrettableDivergences: current.regrettableDivergences - baseline.regrettableDivergences,
    regrettableDivergenceScenarios: current.regrettableDivergenceScenarios - baseline.regrettableDivergenceScenarios,
  };
}

function renderCaptureOnlyLine(
  current: ConformanceSummary,
  baseline: ConformanceSummary | null,
  delta: ConformanceDelta | null,
): string {
  const currentCount = current.captureOnlyScenarios ?? 0;
  const prefix = `- Capture-only scenarios: ${currentCount} not counted as strict parity`;

  if (!baseline || !delta) {
    return prefix;
  }

  if (!delta.captureOnlyScenariosKnown) {
    return `${prefix} (main baseline predates this breakdown)`;
  }

  return `${prefix} (main: ${baseline.captureOnlyScenarios ?? 0}, delta: ${formatSignedInteger(
    delta.captureOnlyScenarios,
  )})`;
}

function renderCaptureOnlyAlarm(delta: ConformanceDelta | null): string | null {
  if (!delta?.captureOnlyScenariosKnown || delta.captureOnlyScenarios <= 0) {
    return null;
  }

  return `- ALARM: capture-only parity specs increased by ${formatSignedInteger(
    delta.captureOnlyScenarios,
  )} vs main. These scenarios are staged evidence, not strict proxy-vs-capture comparisons.`;
}

function renderCaptureOnlyDetails(current: ConformanceSummary): string[] {
  const scenarioIds = [...(current.captureOnlyScenarioIds ?? [])].sort((left, right) => left.localeCompare(right));
  if (scenarioIds.length === 0) {
    return [];
  }

  return [
    '',
    '<details>',
    `<summary>Capture-only parity specs (${scenarioIds.length})</summary>`,
    '',
    ...scenarioIds.map((scenarioId) => `- \`${scenarioId}\``),
    '',
    '</details>',
  ];
}

function renderRuntimeFixtureLine(
  current: ConformanceSummary,
  baseline: ConformanceSummary | null,
  delta: ConformanceDelta | null,
): string {
  const currentCount = current.runtimeFixtureScenarios ?? 0;
  const prefix = `- Runtime-test-backed fixture scenarios: ${currentCount}`;

  if (!baseline || !delta) {
    return prefix;
  }

  return `${prefix} (main: ${baseline.runtimeFixtureScenarios ?? 0}, delta: ${formatSignedInteger(
    delta.runtimeFixtureScenarios,
  )})`;
}

function renderApiSurfaceOperationLines(current: ConformanceSummary): string[] {
  const summaries = current.apiSurfaceSummaries;
  if (!summaries) {
    return [];
  }
  return ['admin', 'storefront'].flatMap((apiSurface) => {
    const summary = summaries[apiSurface];
    if (!summary) {
      return [];
    }
    return [
      `- ${apiSurface === 'admin' ? 'Admin' : 'Storefront'} covered operations: ${summary.coveredOperations} / ${summary.implementedOperations} (${summary.declaredGapOperations} declared gaps)`,
    ];
  });
}

export function renderConformanceComment(report: ConformanceReport): string {
  const baseline = report.baseline;
  const delta = report.delta;
  const current = report.conformance;
  const baselineLine = baseline
    ? `- Main baseline: ${baseline.conformingScenarios} / ${baseline.totalScenarios} scenarios have executable conformance evidence (${formatPercent(baseline.conformanceRatio)})`
    : '- Main baseline: not found yet. The next successful push to `main` will publish one.';
  const improvementLine = delta
    ? `- Improvement over main: ${formatSignedInteger(delta.conformingScenarios)} conformance-evidenced scenarios (${formatSignedPercentPoints(delta.conformanceRatio * 100)})`
    : '- Improvement over main: unavailable until a main baseline artifact exists.';
  const regrettableDivergenceLine =
    baseline && delta
      ? `- Regrettable divergences: ${current.regrettableDivergences} expected differences across ${current.regrettableDivergenceScenarios} scenarios (main: ${baseline.regrettableDivergences}, delta: ${formatSignedInteger(delta.regrettableDivergences)})`
      : `- Regrettable divergences: ${current.regrettableDivergences} expected differences across ${current.regrettableDivergenceScenarios} scenarios`;
  const commit = report.commit ? report.commit.slice(0, 12) : 'unknown';
  const captureOnlyAlarm = renderCaptureOnlyAlarm(delta);

  return [
    commentMarker,
    '## Conformance status',
    '',
    `- Current branch: ${current.conformingScenarios} / ${current.totalScenarios} scenarios have executable conformance evidence (${formatPercent(current.conformanceRatio)})`,
    `- Strict proxy-vs-capture comparisons: ${current.strictComparisonScenarios ?? current.conformingScenarios}`,
    renderRuntimeFixtureLine(current, baseline, delta),
    baselineLine,
    improvementLine,
    renderCaptureOnlyLine(current, baseline, delta),
    ...(captureOnlyAlarm ? [captureOnlyAlarm] : []),
    regrettableDivergenceLine,
    `- Covered operations: ${current.coveredOperations} / ${current.implementedOperations} (${current.declaredGapOperations} declared gaps)`,
    ...renderApiSurfaceOperationLines(current),
    `- Commit: \`${commit}\``,
    ...renderCaptureOnlyDetails(current),
    '',
    '_Generated from convention-discovered conformance parity specs._',
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
  const status =
    statusPath && existsSync(statusPath) ? readConformanceStatus(statusPath) : buildConformanceStatusDocument(repoRoot);
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
    `conformance status: ${report.conformance.conformingScenarios}/${report.conformance.totalScenarios} scenarios have executable conformance evidence`,
  );
  writeLine(`strict proxy-vs-capture comparisons: ${report.conformance.strictComparisonScenarios ?? 0}`);
  writeLine(`runtime-test-backed fixture scenarios: ${report.conformance.runtimeFixtureScenarios ?? 0}`);
  writeLine(`capture-only scenarios: ${report.conformance.captureOnlyScenarios ?? 0}`);
  for (const line of renderApiSurfaceOperationLines(report.conformance)) {
    writeLine(line.slice(2));
  }
  if (report.delta) {
    writeLine(
      `improvement over main: ${formatSignedInteger(report.delta.conformingScenarios)} conformance-evidenced scenarios`,
    );
  } else {
    writeLine('improvement over main: baseline unavailable');
  }
}

function printHelp(): void {
  process.stdout.write(`Usage: tsx scripts/conformance-status-report.ts [options]

Options:
  --status-json <path>       Optional source conformance status JSON. Defaults to discovered parity specs.
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
      statusPath: args.has('status-json') ? path.resolve(repoRoot, args.get('status-json') ?? '') : null,
      baselinePath: args.has('baseline') ? path.resolve(repoRoot, args.get('baseline') ?? '') : null,
      outputJsonPath: path.resolve(repoRoot, args.get('output-json') ?? defaultOutputJsonPath),
      outputMarkdownPath: path.resolve(repoRoot, args.get('output-markdown') ?? defaultOutputMarkdownPath),
    });
  }
}
