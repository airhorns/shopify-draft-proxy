import { readFileSync } from 'node:fs';
import path from 'node:path';

import { parseOperation } from '../src/graphql/parse-operation.js';
import { getOperationCapability } from '../src/proxy/capabilities.js';
import { handleProductMutation, handleProductQuery } from '../src/proxy/products.js';
import { makeSyntheticGid, makeSyntheticTimestamp, resetSyntheticIdentity } from '../src/state/synthetic-identity.js';
import { store } from '../src/state/store.js';

export type ParityScenarioState =
  | 'ready-for-comparison'
  | 'captured-awaiting-proxy-request'
  | 'captured-awaiting-comparison-contract'
  | 'planned-with-proxy-request'
  | 'planned';

type Matcher = 'any-string' | 'non-empty-string' | 'any-number' | 'iso-timestamp' | `shopify-gid:${string}`;

export interface AllowedDifference {
  path: string;
  ignore?: boolean;
  matcher?: Matcher;
  reason?: string;
}

export interface ProxyRequestSpec {
  documentPath?: string | null;
  variablesPath?: string | null;
  variables?: Record<string, unknown>;
}

export interface ComparisonTarget {
  name: string;
  capturePath: string;
  proxyPath: string;
  proxyRequest?: ProxyRequestSpec;
}

export interface ComparisonContract {
  mode?: string | null;
  allowedDifferences?: AllowedDifference[] | null;
  targets?: ComparisonTarget[] | null;
}

export interface ParitySpec {
  proxyRequest?: ProxyRequestSpec;
  comparison?: ComparisonContract;
  liveCaptureFiles?: string[];
}

export interface Scenario {
  id: string;
  status: string;
  operationNames?: string[];
  assertionKinds?: string[];
  captureFiles?: string[];
  paritySpecPath?: string;
}

interface Difference {
  path: string;
  message: string;
  expected: unknown;
  actual: unknown;
}

interface CompiledRule extends AllowedDifference {
  segments: PathSegment[];
}

type PathSegment = string | number | '*';

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasProxyRequest(paritySpec: ParitySpec | null | undefined): boolean {
  return !!paritySpec?.proxyRequest?.documentPath;
}

function hasComparisonContract(paritySpec: ParitySpec | null | undefined): boolean {
  return validateComparisonContract(paritySpec?.comparison).length === 0;
}

function isKnownMatcher(matcher: string): matcher is Matcher {
  return (
    matcher === 'any-string' ||
    matcher === 'non-empty-string' ||
    matcher === 'any-number' ||
    matcher === 'iso-timestamp' ||
    /^shopify-gid:([A-Za-z][A-Za-z0-9]*)$/.test(matcher)
  );
}

export function validateComparisonContract(comparison: unknown): string[] {
  const errors: string[] = [];
  const candidate = isPlainObject(comparison) ? comparison : {};

  if (candidate['mode'] !== 'strict-json') {
    errors.push('Comparison contract mode must be `strict-json`.');
  }

  if (!Array.isArray(candidate['allowedDifferences'])) {
    errors.push('Comparison contract must declare an `allowedDifferences` array.');
    return errors;
  }

  for (const [index, rawRule] of candidate['allowedDifferences'].entries()) {
    const rule = isPlainObject(rawRule) ? rawRule : {};
    const label = `allowedDifferences[${index}]`;
    if (typeof rule['path'] !== 'string' || rule['path'].length === 0) {
      errors.push(`${label} must declare a non-empty JSON path.`);
    }

    if (typeof rule['reason'] !== 'string' || rule['reason'].length === 0) {
      errors.push(`${label} must document why the difference is nondeterministic.`);
    }

    const hasMatcher = typeof rule['matcher'] === 'string';
    const isIgnored = rule['ignore'] === true;
    if (hasMatcher === isIgnored) {
      errors.push(`${label} must declare exactly one of \`matcher\` or \`ignore: true\`.`);
    }

    if (hasMatcher && !isKnownMatcher(rule['matcher'] as string)) {
      errors.push(`${label} declares unknown matcher \`${String(rule['matcher'])}\`.`);
    }
  }

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
        if (typeof target['proxyPath'] !== 'string' || target['proxyPath'].length === 0) {
          errors.push(`${label} must declare a non-empty proxyPath.`);
        }
      }
    }
  }

  return errors;
}

export function classifyParityScenarioState(scenario: Pick<Scenario, 'status'>, paritySpec: ParitySpec | null | undefined): ParityScenarioState {
  if (scenario.status === 'captured') {
    if (!hasProxyRequest(paritySpec)) {
      return 'captured-awaiting-proxy-request';
    }

    return hasComparisonContract(paritySpec) ? 'ready-for-comparison' : 'captured-awaiting-comparison-contract';
  }

  return hasProxyRequest(paritySpec) ? 'planned-with-proxy-request' : 'planned';
}

export const parityStatusNote =
  'readyForComparison now means a captured scenario has a proxy request and an explicit strict-json comparison contract. capturedAwaitingComparisonContract scenarios are not parity failures; they are captured scenarios that need scoped nondeterminism rules before payload comparison is meaningful.';

export function summarizeParityResults(results: Array<{ state: ParityScenarioState }>): {
  readyForComparison: number;
  pending: number;
  statusCounts: Record<'readyForComparison' | 'capturedAwaitingComparisonContract' | 'capturedAwaitingProxyRequest' | 'plannedWithProxyRequest' | 'planned', number>;
  statusNote: string;
} {
  const readyForComparison = results.filter((result) => result.state === 'ready-for-comparison').length;
  const capturedAwaitingComparisonContract = results.filter((result) => result.state === 'captured-awaiting-comparison-contract').length;
  const capturedAwaitingProxyRequest = results.filter((result) => result.state === 'captured-awaiting-proxy-request').length;
  const plannedWithProxyRequest = results.filter((result) => result.state === 'planned-with-proxy-request').length;
  const planned = results.filter((result) => result.state === 'planned').length;

  return {
    readyForComparison,
    pending: results.length - readyForComparison,
    statusCounts: {
      readyForComparison,
      capturedAwaitingComparisonContract,
      capturedAwaitingProxyRequest,
      plannedWithProxyRequest,
      planned,
    },
    statusNote: parityStatusNote,
  };
}

function appendPath(currentPath: string, segment: string | number): string {
  if (typeof segment === 'number') {
    return `${currentPath}[${segment}]`;
  }

  if (/^[A-Za-z_$][\w$]*$/.test(segment)) {
    return `${currentPath}.${segment}`;
  }

  return `${currentPath}[${JSON.stringify(segment)}]`;
}

function parsePath(pathValue: string): PathSegment[] {
  if (!pathValue.startsWith('$')) {
    throw new Error(`Invalid comparison path: ${pathValue}`);
  }

  const segments: PathSegment[] = [];
  let index = 1;
  while (index < pathValue.length) {
    if (pathValue[index] === '.') {
      index += 1;
      const match = /^[A-Za-z_$][\w$]*/.exec(pathValue.slice(index));
      if (!match?.[0]) {
        throw new Error(`Invalid comparison path segment in: ${pathValue}`);
      }
      segments.push(match[0]);
      index += match[0].length;
      continue;
    }

    if (pathValue[index] === '[') {
      const closeIndex = pathValue.indexOf(']', index);
      if (closeIndex === -1) {
        throw new Error(`Invalid comparison path segment in: ${pathValue}`);
      }
      const raw = pathValue.slice(index + 1, closeIndex);
      if (raw === '*') {
        segments.push('*');
      } else if (/^\d+$/.test(raw)) {
        segments.push(Number.parseInt(raw, 10));
      } else {
        segments.push(JSON.parse(raw) as string);
      }
      index = closeIndex + 1;
      continue;
    }

    throw new Error(`Invalid comparison path segment in: ${pathValue}`);
  }

  return segments;
}

function pathMatches(ruleSegments: PathSegment[], pathSegments: PathSegment[]): boolean {
  if (ruleSegments.length !== pathSegments.length) {
    return false;
  }

  return ruleSegments.every((segment, index) => segment === '*' || segment === pathSegments[index]);
}

function makeRule(rawRule: AllowedDifference): CompiledRule {
  return {
    ...rawRule,
    segments: parsePath(rawRule.path),
  };
}

function findRule(rules: CompiledRule[], pathSegments: PathSegment[]): CompiledRule | null {
  return rules.find((rule) => pathMatches(rule.segments, pathSegments)) ?? null;
}

function isIsoTimestamp(value: unknown): boolean {
  if (typeof value !== 'string') {
    return false;
  }

  const parsed = Date.parse(value);
  return Number.isFinite(parsed) && new Date(parsed).toISOString() === value;
}

function isShopifyGid(value: unknown, resourceType: string): boolean {
  return typeof value === 'string' && value.startsWith(`gid://shopify/${resourceType}/`) && value.length > `gid://shopify/${resourceType}/`.length;
}

function matcherAccepts(matcher: Matcher, expected: unknown, actual: unknown): boolean {
  if (matcher === 'any-string') {
    return typeof expected === 'string' && typeof actual === 'string';
  }

  if (matcher === 'non-empty-string') {
    return typeof expected === 'string' && expected.length > 0 && typeof actual === 'string' && actual.length > 0;
  }

  if (matcher === 'any-number') {
    return typeof expected === 'number' && typeof actual === 'number';
  }

  if (matcher === 'iso-timestamp') {
    return isIsoTimestamp(expected) && isIsoTimestamp(actual);
  }

  const gidMatch = /^shopify-gid:([A-Za-z][A-Za-z0-9]*)$/.exec(matcher);
  if (gidMatch?.[1]) {
    return isShopifyGid(expected, gidMatch[1]) && isShopifyGid(actual, gidMatch[1]);
  }

  throw new Error(`Unknown comparison matcher: ${matcher}`);
}

function diffValues(expected: unknown, actual: unknown, currentPath: string, pathSegments: PathSegment[], rules: CompiledRule[], differences: Difference[]): void {
  const rule = findRule(rules, pathSegments);
  if (rule?.ignore === true) {
    return;
  }

  if (Object.is(expected, actual)) {
    return;
  }

  if (rule?.matcher && matcherAccepts(rule.matcher, expected, actual)) {
    return;
  }

  if (Array.isArray(expected) || Array.isArray(actual)) {
    if (!Array.isArray(expected) || !Array.isArray(actual)) {
      differences.push({ path: currentPath, message: 'Expected both values to be arrays.', expected, actual });
      return;
    }

    if (expected.length !== actual.length) {
      differences.push({ path: currentPath, message: `Array length differs: expected ${expected.length}, received ${actual.length}.`, expected, actual });
      return;
    }

    for (let index = 0; index < expected.length; index += 1) {
      diffValues(expected[index], actual[index], appendPath(currentPath, index), [...pathSegments, index], rules, differences);
    }
    return;
  }

  if (isPlainObject(expected) || isPlainObject(actual)) {
    if (!isPlainObject(expected) || !isPlainObject(actual)) {
      differences.push({ path: currentPath, message: 'Expected both values to be objects.', expected, actual });
      return;
    }

    const keys = new Set([...Object.keys(expected), ...Object.keys(actual)]);
    for (const key of [...keys].sort()) {
      const childPath = appendPath(currentPath, key);
      const childSegments = [...pathSegments, key];
      const childRule = findRule(rules, childSegments);

      if (childRule?.ignore === true) {
        continue;
      }

      if (!Object.prototype.hasOwnProperty.call(expected, key)) {
        differences.push({ path: childPath, message: 'Unexpected field in actual payload.', expected: undefined, actual: actual[key] });
        continue;
      }

      if (!Object.prototype.hasOwnProperty.call(actual, key)) {
        differences.push({ path: childPath, message: 'Missing field in actual payload.', expected: expected[key], actual: undefined });
        continue;
      }

      diffValues(expected[key], actual[key], childPath, childSegments, rules, differences);
    }
    return;
  }

  differences.push({ path: currentPath, message: 'Value differs.', expected, actual });
}

export function compareJsonPayloads(expected: unknown, actual: unknown, comparison: Pick<ComparisonContract, 'allowedDifferences'> = {}): { ok: boolean; differences: Difference[] } {
  const allowedDifferences = Array.isArray(comparison.allowedDifferences) ? comparison.allowedDifferences : [];
  const rules = allowedDifferences.map(makeRule);
  const differences: Difference[] = [];

  diffValues(expected, actual, '$', [], rules, differences);

  return {
    ok: differences.length === 0,
    differences,
  };
}

function readJsonFile(repoRoot: string, relativePath: string): unknown {
  return JSON.parse(readFileSync(path.join(repoRoot, relativePath), 'utf8')) as unknown;
}

function readTextFile(repoRoot: string, relativePath: string): string {
  return readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

export function readJsonPath(value: unknown, pathValue: string): unknown {
  let current = value;
  for (const segment of parsePath(pathValue)) {
    if (segment === '*') {
      throw new Error(`Wildcard is not supported when reading a single JSON path: ${pathValue}`);
    }
    if (current === null || typeof current !== 'object') {
      return undefined;
    }
    current = (current as Record<string | number, unknown>)[segment];
  }
  return current;
}

function materializeValue(rawValue: unknown, primaryProxyResponse: unknown): unknown {
  if (Array.isArray(rawValue)) {
    return rawValue.map((item) => materializeValue(item, primaryProxyResponse));
  }

  if (!isPlainObject(rawValue)) {
    return rawValue;
  }

  if (typeof rawValue['fromPrimaryProxyPath'] === 'string') {
    return readJsonPath(primaryProxyResponse, rawValue['fromPrimaryProxyPath']);
  }

  return Object.fromEntries(Object.entries(rawValue).map(([key, value]) => [key, materializeValue(value, primaryProxyResponse)]));
}

function materializeVariables(rawVariables: unknown, primaryProxyResponse: unknown): Record<string, unknown> {
  const materialized = materializeValue(rawVariables ?? {}, primaryProxyResponse);
  return isPlainObject(materialized) ? materialized : {};
}

async function executeGraphQLAgainstLocalProxy(document: string, variables: Record<string, unknown>): Promise<{ status: number; body: Record<string, unknown> }> {
  const parsed = parseOperation(document);
  const capability = getOperationCapability(parsed);

  if (capability.execution === 'stage-locally' && capability.domain === 'products') {
    store.appendLog({
      id: makeSyntheticGid('MutationLogEntry'),
      receivedAt: makeSyntheticTimestamp(),
      operationName: capability.operationName,
      query: document,
      variables,
      status: 'staged',
      notes: 'Staged locally in the conformance parity proxy harness.',
    });

    return {
      status: 200,
      body: handleProductMutation(document, variables),
    };
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'products') {
    return {
      status: 200,
      body: handleProductQuery(document, variables, 'snapshot'),
    };
  }

  throw new Error(`Parity execution does not allow live Shopify requests or unsupported operations: ${capability.operationName}`);
}

function readComparisonTargets(comparison: ComparisonContract): ComparisonTarget[] {
  if (Array.isArray(comparison.targets) && comparison.targets.length > 0) {
    return comparison.targets;
  }

  return [
    {
      name: 'primary-response',
      capturePath: '$.mutation.response',
      proxyPath: '$',
    },
  ];
}

export async function executeParityScenario({ repoRoot, scenario, paritySpec }: { repoRoot: string; scenario: Scenario; paritySpec: ParitySpec }): Promise<{
  ok: boolean;
  primaryProxyStatus: number;
  comparisons: Array<{ name: string; ok: boolean; differences: Difference[] }>;
}> {
  if (!paritySpec.proxyRequest?.documentPath) {
    throw new Error(`Scenario ${scenario.id} does not define a proxy request.`);
  }
  if (validateComparisonContract(paritySpec.comparison).length > 0 || !paritySpec.comparison) {
    throw new Error(`Scenario ${scenario.id} does not define a valid comparison contract.`);
  }

  store.reset();
  resetSyntheticIdentity();

  const capturePath = scenario.captureFiles?.[0] ?? paritySpec.liveCaptureFiles?.[0];
  if (typeof capturePath !== 'string') {
    throw new Error(`Scenario ${scenario.id} does not reference a capture fixture.`);
  }

  const capture = readJsonFile(repoRoot, capturePath);
  const primaryDocument = readTextFile(repoRoot, paritySpec.proxyRequest.documentPath);
  const primaryVariables = paritySpec.proxyRequest.variablesPath
    ? readJsonFile(repoRoot, paritySpec.proxyRequest.variablesPath)
    : {};
  const primaryProxyResponse = await executeGraphQLAgainstLocalProxy(primaryDocument, materializeVariables(primaryVariables, {}));

  const comparisons = [];
  for (const target of readComparisonTargets(paritySpec.comparison)) {
    const expected = readJsonPath(capture, target.capturePath);
    let proxyResponseBody: unknown = primaryProxyResponse.body;

    if (target.proxyRequest?.documentPath) {
      const document = readTextFile(repoRoot, target.proxyRequest.documentPath);
      const variables = target.proxyRequest.variablesPath
        ? readJsonFile(repoRoot, target.proxyRequest.variablesPath)
        : target.proxyRequest.variables;
      const proxyResponse = await executeGraphQLAgainstLocalProxy(document, materializeVariables(variables, primaryProxyResponse.body));
      proxyResponseBody = proxyResponse.body;
    }

    const actual = readJsonPath(proxyResponseBody, target.proxyPath);
    const comparison = compareJsonPayloads(expected, actual, paritySpec.comparison);
    comparisons.push({
      name: target.name,
      ok: comparison.ok,
      differences: comparison.differences,
    });
  }

  return {
    ok: comparisons.every((comparison) => comparison.ok),
    primaryProxyStatus: primaryProxyResponse.status,
    comparisons,
  };
}
