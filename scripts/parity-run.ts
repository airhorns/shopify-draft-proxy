import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { readFile, readdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createDraftProxy, type DraftProxy } from '../js/src/index.js';

type CliArgs = {
  all: boolean;
  debug: boolean;
  dryRun: boolean;
  scenarioIds: string[];
  specPaths: string[];
};

type ProxyRequestSpec = {
  documentPath?: string;
  documentCapturePath?: string;
  variables?: Record<string, unknown>;
  variablesPath?: string;
  variablesCapturePath?: string;
  headers?: Record<string, string>;
};

type ComparisonTarget = {
  name: string;
  capturePath: string;
  proxyPath?: string;
  proxyStatePath?: string;
  proxyLogPath?: string;
  proxyRequest?: ProxyRequestSpec;
  selectedPaths?: string[];
  excludedPaths?: string[];
  expectedDifferences?: ExpectedDifference[];
};

type ExpectedDifference = {
  path: string;
  matcher?: string;
  ignore?: true;
  reason: string;
};

type ParitySpec = {
  scenarioId: string;
  liveCaptureFiles?: string[];
  proxyRequest?: ProxyRequestSpec;
  comparison?: {
    expectedDifferences?: ExpectedDifference[];
    targets?: ComparisonTarget[];
  };
};

type RecordedUpstreamCall = {
  operationName?: string;
  variables?: unknown;
  query?: string;
  response?: { status?: number; body?: unknown };
};

type ProxyResponse = { status: number; body: unknown };

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const paritySpecRoot = path.join(repoRoot, 'config', 'parity-specs');
const adminPath = '/admin/api/2026-04/graphql.json';

function log(message: string): void {
  process.stdout.write(`${message}\n`);
}

function logError(message: string): void {
  process.stderr.write(`${message}\n`);
}

function parseArgs(argv: string[]): CliArgs {
  const args: CliArgs = { all: false, debug: false, dryRun: false, scenarioIds: [], specPaths: [] };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index] ?? '';
    if (arg === '--') continue;
    if (arg === '--all') args.all = true;
    else if (arg === '--debug') args.debug = true;
    else if (arg === '--dry-run') args.dryRun = true;
    else if (arg === '--spec') {
      const next = argv[index + 1];
      if (!next || next.startsWith('-')) throw new Error('--spec requires a path argument');
      args.specPaths.push(next);
      index += 1;
    } else if (arg.startsWith('-')) throw new Error(`Unknown flag: ${arg}`);
    else args.scenarioIds.push(arg);
  }
  return args;
}

async function findAllSpecPaths(directory = paritySpecRoot): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async (entry) => {
      const fullPath = path.join(directory, entry.name);
      if (entry.isDirectory()) return await findAllSpecPaths(fullPath);
      return entry.isFile() && entry.name.endsWith('.json') ? [fullPath] : [];
    }),
  );
  return nested.flat().sort();
}

async function readJsonFile<T>(filePath: string): Promise<T> {
  return JSON.parse(await readFile(filePath, 'utf8')) as T;
}

async function findSpecForScenario(scenarioId: string): Promise<string> {
  for (const specPath of await findAllSpecPaths()) {
    try {
      const parsed = await readJsonFile<{ scenarioId?: string }>(specPath);
      if (parsed.scenarioId === scenarioId) return specPath;
    } catch {
      // conformance checks report invalid JSON with better context.
    }
  }
  throw new Error(`No parity spec with scenarioId "${scenarioId}" found under config/parity-specs/`);
}

async function resolveSpecPaths(args: CliArgs): Promise<string[]> {
  if (args.all) return await findAllSpecPaths();
  const specPaths: string[] = [];
  for (const scenarioId of args.scenarioIds) specPaths.push(await findSpecForScenario(scenarioId));
  for (const specPath of args.specPaths) specPaths.push(path.isAbsolute(specPath) ? specPath : path.resolve(repoRoot, specPath));
  return specPaths;
}

function tokenizeJsonPath(jsonPath: string): string[] {
  if (!jsonPath.startsWith('$')) throw new Error(`Unsupported JSONPath (must start with $): ${jsonPath}`);
  const parts: string[] = [];
  const pattern = /\.([^.[\]]+)|\[(\d+)\]/gu;
  for (const match of jsonPath.matchAll(pattern)) parts.push(match[1] ?? match[2] ?? '');
  return parts;
}

function getPath(value: unknown, jsonPath: string): unknown {
  let cursor = value;
  for (const part of tokenizeJsonPath(jsonPath)) {
    if (Array.isArray(cursor)) cursor = cursor[Number(part)];
    else if (typeof cursor === 'object' && cursor !== null) cursor = (cursor as Record<string, unknown>)[part];
    else return undefined;
  }
  return cursor;
}

function setPath(root: unknown, jsonPath: string, value: unknown): unknown {
  const parts = tokenizeJsonPath(jsonPath);
  if (parts.length === 0) return value;
  const out: Record<string, unknown> = {};
  let cursor: Record<string, unknown> = out;
  for (const [index, part] of parts.entries()) {
    if (index === parts.length - 1) cursor[part] = value;
    else {
      const next: Record<string, unknown> = {};
      cursor[part] = next;
      cursor = next;
    }
  }
  return root === undefined ? out : out;
}

function selectPaths(value: unknown, paths: string[] | undefined): unknown {
  if (!paths || paths.length === 0) return value;
  let out: unknown = undefined;
  for (const jsonPath of paths) out = setPath(out, jsonPath, getPath(value, jsonPath));
  return out;
}

function deepClone<T>(value: T): T {
  return value === undefined ? value : (JSON.parse(JSON.stringify(value)) as T);
}

function deletePath(root: unknown, jsonPath: string): unknown {
  const copy = deepClone(root);
  const parts = tokenizeJsonPath(jsonPath);
  if (parts.length === 0) return undefined;
  let cursor: unknown = copy;
  for (const part of parts.slice(0, -1)) {
    cursor = Array.isArray(cursor) ? cursor[Number(part)] : (cursor as Record<string, unknown> | undefined)?.[part];
    if (cursor === undefined || cursor === null) return copy;
  }
  const last = parts[parts.length - 1] ?? '';
  if (Array.isArray(cursor)) cursor.splice(Number(last), 1);
  else if (typeof cursor === 'object' && cursor !== null) delete (cursor as Record<string, unknown>)[last];
  return copy;
}

function applyExcludedPaths(value: unknown, paths: string[] | undefined): unknown {
  let out = value;
  for (const jsonPath of paths ?? []) out = deletePath(out, jsonPath);
  return out;
}

function resolveSpecialVariables(value: unknown, primaryResponse: ProxyResponse | null): unknown {
  if (Array.isArray(value)) return value.map((entry) => resolveSpecialVariables(entry, primaryResponse));
  if (typeof value === 'object' && value !== null) {
    const object = value as Record<string, unknown>;
    if (typeof object['fromPrimaryProxyPath'] === 'string') {
      if (primaryResponse === null) throw new Error('fromPrimaryProxyPath used before primary proxy response exists');
      return getPath(primaryResponse.body, object['fromPrimaryProxyPath']);
    }
    return Object.fromEntries(Object.entries(object).map(([key, entry]) => [key, resolveSpecialVariables(entry, primaryResponse)]));
  }
  return value;
}

async function loadRequest(
  request: ProxyRequestSpec | undefined,
  capture: unknown,
  primaryResponse: ProxyResponse | null,
): Promise<{ query: string; variables: Record<string, unknown>; headers: Record<string, string> } | null> {
  if (!request || (!request.documentPath && !request.documentCapturePath)) return null;
  let query: string;
  if (request.documentCapturePath) {
    const document = getPath(capture, request.documentCapturePath);
    if (typeof document !== 'string') throw new Error(`Spec references missing captured document: ${request.documentCapturePath}`);
    query = document;
  } else {
    const documentPath = path.resolve(repoRoot, request.documentPath ?? '');
    if (!existsSync(documentPath)) throw new Error(`Spec references missing document: ${request.documentPath ?? ''}`);
    query = await readFile(documentPath, 'utf8');
  }

  let variables: Record<string, unknown> = {};
  if (request.variablesCapturePath) variables = (getPath(capture, request.variablesCapturePath) ?? {}) as Record<string, unknown>;
  else if (request.variablesPath) variables = await readJsonFile(path.resolve(repoRoot, request.variablesPath));
  else if (request.variables) variables = request.variables;

  variables = resolveSpecialVariables(variables, primaryResponse) as Record<string, unknown>;
  return { query, variables, headers: request.headers ?? {} };
}

type CassetteServer = {
  origin: string;
  setCalls: (calls: RecordedUpstreamCall[]) => void;
  consumed: () => number;
  expected: () => number;
  close: () => Promise<void>;
};

async function startCassetteServer(): Promise<CassetteServer> {
  let calls: RecordedUpstreamCall[] = [];
  let index = 0;
  const server = createServer((request: IncomingMessage, response: ServerResponse) => {
    let body = '';
    request.setEncoding('utf8');
    request.on('data', (chunk) => (body += chunk));
    request.on('end', () => {
      const call = calls[index++];
      if (!call) {
        response.statusCode = 500;
        response.setHeader('content-type', 'application/json');
        response.end(JSON.stringify({ errors: [{ message: `Unexpected upstream call ${index}: ${body}` }] }));
        return;
      }
      response.statusCode = call.response?.status ?? 200;
      response.setHeader('content-type', 'application/json');
      response.end(JSON.stringify(call.response?.body ?? {}));
    });
  });
  await new Promise<void>((resolveListen) => server.listen(0, '127.0.0.1', resolveListen));
  const address = server.address();
  if (address === null || typeof address === 'string') throw new Error('Failed to start cassette server');
  return {
    origin: `http://127.0.0.1:${address.port}`,
    setCalls: (nextCalls: RecordedUpstreamCall[]) => {
      calls = nextCalls;
      index = 0;
    },
    consumed: () => index,
    expected: () => calls.length,
    close: async () => await new Promise<void>((resolveClose, reject) => server.close((error) => (error ? reject(error) : resolveClose()))),
  };
}

async function sendProxyRequest(proxy: DraftProxy, request: { query: string; variables: Record<string, unknown>; headers: Record<string, string> }): Promise<ProxyResponse> {
  return await proxy.processRequest({
    method: 'POST',
    path: adminPath,
    headers: { 'content-type': 'application/json', ...request.headers },
    body: { query: request.query, variables: request.variables },
  });
}

function normalizeForTarget(value: unknown, target: ComparisonTarget): unknown {
  return applyExcludedPaths(selectPaths(value, target.selectedPaths), target.excludedPaths);
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function matchesRule(value: unknown, rule: ExpectedDifference): boolean {
  if (rule.ignore) return true;
  const matcher = rule.matcher ?? '';
  if (matcher === 'any-string') return typeof value === 'string';
  if (matcher === 'non-empty-string') return typeof value === 'string' && value.length > 0;
  if (matcher === 'any-number') return typeof value === 'number';
  if (matcher === 'iso-timestamp') return typeof value === 'string' && /^\d{4}-\d{2}-\d{2}T/u.test(value);
  if (matcher === 'storefront-access-token') return typeof value === 'string' && value.length > 0;
  const gidMatch = /^shopify-gid:([A-Za-z][A-Za-z0-9]*)$/u.exec(matcher);
  if (gidMatch) return typeof value === 'string' && value.startsWith(`gid://shopify/${gidMatch[1]}/`);
  if (matcher.startsWith('exact-string:')) return value === matcher.slice('exact-string:'.length);
  if (matcher.startsWith('regex:')) return typeof value === 'string' && new RegExp(matcher.slice('regex:'.length), 'u').test(value);
  if (matcher.startsWith('shop-policy-url-base:')) return typeof value === 'string' && value.startsWith(matcher.slice('shop-policy-url-base:'.length));
  return false;
}

function diffValues(capture: unknown, proxy: unknown, rules: ExpectedDifference[], basePath = '$'): string[] {
  const rule = rules.find((candidate) => candidate.path === basePath);
  if (rule && matchesRule(proxy, rule)) return [];
  if (Object.is(capture, proxy)) return [];
  if (Array.isArray(capture) && Array.isArray(proxy)) {
    const errors: string[] = [];
    const max = Math.max(capture.length, proxy.length);
    for (let index = 0; index < max; index += 1) errors.push(...diffValues(capture[index], proxy[index], rules, `${basePath}[${index}]`));
    return errors;
  }
  if (isPlainObject(capture) && isPlainObject(proxy)) {
    const errors: string[] = [];
    const keys = new Set([...Object.keys(capture), ...Object.keys(proxy)]);
    for (const key of [...keys].sort()) errors.push(...diffValues(capture[key], proxy[key], rules, `${basePath}.${key}`));
    return errors;
  }
  return [`${basePath}: expected ${JSON.stringify(capture)} got ${JSON.stringify(proxy)}`];
}

async function runSpec(specPath: string, debug: boolean, proxy: DraftProxy, cassette: CassetteServer): Promise<string[]> {
  const relativeSpecPath = path.relative(repoRoot, specPath);
  const spec = await readJsonFile<ParitySpec>(specPath);
  const capturePath = spec.liveCaptureFiles?.[0];
  if (!capturePath) return [`${relativeSpecPath}: spec has no liveCaptureFiles[0]`];
  const capture = await readJsonFile<Record<string, unknown>>(path.resolve(repoRoot, capturePath));
  const upstreamCalls = (capture['upstreamCalls'] ?? []) as RecordedUpstreamCall[];
  cassette.setCalls(upstreamCalls);
  await proxy.processRequest({ method: 'POST', path: '/__meta/reset' });
  const failures: string[] = [];
  let primaryResponse: ProxyResponse | null = null;
  try {
    const primaryRequest = await loadRequest(spec.proxyRequest, capture, null);
    if (primaryRequest !== null) primaryResponse = await sendProxyRequest(proxy, primaryRequest);

    for (const target of spec.comparison?.targets ?? []) {
      let proxySource: unknown;
      if (target.proxyRequest) {
        const request = await loadRequest(target.proxyRequest, capture, primaryResponse);
        if (request === null) throw new Error(`${target.name}: target proxyRequest did not resolve to a request`);
        proxySource = (await sendProxyRequest(proxy, request)).body;
        if (debug) log(`[parity-debug] ${relativeSpecPath} [${target.name}] proxy response ${JSON.stringify(proxySource).slice(0, 1000)}`);
      } else if (target.proxyStatePath) {
        proxySource = await proxy.getState();
      } else if (target.proxyLogPath) {
        proxySource = await proxy.getLog();
      } else {
        proxySource = primaryResponse?.body;
      }
      const captureValue = normalizeForTarget(getPath(capture, target.capturePath), target);
      const proxyPath = target.proxyPath ?? target.proxyStatePath ?? target.proxyLogPath ?? '$';
      const proxyValue = normalizeForTarget(getPath(proxySource, proxyPath), target);
      const rules = [...(spec.comparison?.expectedDifferences ?? []), ...(target.expectedDifferences ?? [])];
      const diffs = diffValues(captureValue, proxyValue, rules);
      if (diffs.length > 0) {
        failures.push(`${relativeSpecPath} [${target.name}] ${diffs.slice(0, debug ? 20 : 3).join('; ')}`);
      }
    }
    if (cassette.consumed() !== cassette.expected()) {
      failures.push(`${relativeSpecPath}: consumed ${cassette.consumed()}/${cassette.expected()} upstream cassette calls`);
    }
  } catch (error) {
    failures.push(`${relativeSpecPath}: ${(error as Error).stack ?? (error as Error).message}`);
  }
  return failures;
}

async function main(): Promise<void> {
  let args: CliArgs;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (error) {
    logError((error as Error).message);
    logError('Usage: pnpm parity <scenario-id> | --spec <path> | --all [--debug] [--dry-run]');
    process.exit(2);
    return;
  }
  if (!args.all && args.scenarioIds.length === 0 && args.specPaths.length === 0) {
    logError('Usage: pnpm parity <scenario-id> | --spec <path> | --all [--debug] [--dry-run]');
    process.exit(2);
    return;
  }

  const specPaths = await resolveSpecPaths(args);
  log(`[parity] ${specPaths.length} spec(s) selected`);
  if (args.dryRun) return;

  const cassette = await startCassetteServer();
  const proxy = createDraftProxy({
    readMode: 'live-hybrid',
    unsupportedMutationMode: 'passthrough',
    shopifyAdminOrigin: cassette.origin,
    port: 0,
  });

  let failures = 0;
  try {
    for (const specPath of specPaths) {
      const errors = await runSpec(specPath, args.debug, proxy, cassette);
      if (errors.length > 0) {
        failures += 1;
        for (const error of errors) logError(`[parity] ${error}`);
      } else {
        log(`[parity] ${path.relative(repoRoot, specPath)} passed`);
      }
    }
  } finally {
    proxy.dispose();
    await cassette.close();
  }
  if (failures > 0) {
    logError(`[parity] ${failures}/${specPaths.length} spec(s) failed`);
    process.exit(1);
  }
}

await main();
