import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { existsSync, appendFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import {
  createDraftProxy,
  type DraftProxy,
  type DraftProxyRequest,
  type DraftProxyStateDump,
  type ReadMode,
} from '../js/src/index.js';
import {
  apiSurfaceFromGraphqlPath,
  type ApiSurface,
  type OutgoingGraphqlRequest,
  type RecordedUpstreamCall,
  recordedCallMatchesRequest,
  formatRecordedCallMismatch,
  stableJson,
  validateRecordedUpstreamCalls,
} from './parity-cassette.js';
import { DEFAULT_ADMIN_API_VERSION, EXECUTABLE_ADMIN_API_VERSIONS } from './support/shopify/api-version.js';

type CliArgs = {
  all: boolean;
  debug: boolean;
  dryRun: boolean;
  outputJsonPath?: string;
  scenarioIds: string[];
  specPaths: string[];
};

type ProxyRequestSpec = {
  documentPath?: string;
  documentCapturePath?: string;
  operationName?: string | null;
  operationNameCapturePath?: string;
  variables?: Record<string, unknown>;
  variablesPath?: string;
  variablesCapturePath?: string;
  apiSurface?: ApiSurface;
  apiVersion?: string;
  headers?: Record<string, string>;
};

type ProxyUploadSpec = {
  method?: string;
  path: unknown;
  body?: unknown;
  headers?: Record<string, string>;
};

type ProxySetupSpec = {
  name: string;
  captureResponsePath: string;
  proxyRequest: ProxyRequestSpec;
};

type ProxyHttpRequestSpec = {
  method?: string;
  path: unknown;
  body?: unknown;
  headers?: Record<string, string>;
};

type ComparisonTarget = {
  name: string;
  capturePath: string;
  proxyPath?: string;
  proxyStatePath?: string;
  proxyLogPath?: string;
  proxyRequest?: ProxyRequestSpec;
  proxyUpload?: ProxyUploadSpec;
  proxyHttpRequest?: ProxyHttpRequestSpec;
  isolatedProxy?: boolean;
  jsonlRecords?: boolean;
  selectedPaths?: string[];
  excludedPaths?: string[];
  expectedDifferences?: ExpectedDifference[];
  preserveProxyState?: boolean;
  rewriteGidAliases?: boolean;
  preferTargetFallback?: boolean;
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
  proxyConfig?: {
    readMode?: ReadMode;
  };
  proxySetups?: ProxySetupSpec[];
  proxyRequest?: ProxyRequestSpec;
  comparison?: {
    expectedDifferences?: ExpectedDifference[];
    targets?: ComparisonTarget[];
  };
};

type ProxyContext = {
  proxy: DraftProxy;
  cleanState: DraftProxyStateDump;
};

type ProxyResponse = { status: number; body: unknown };

function capturedSchemaProbeCall(capture: Record<string, unknown>): RecordedUpstreamCall[] {
  const probe = capture['schemaProbe'];
  if (typeof probe !== 'object' || probe === null) return [];
  const request = (probe as Record<string, unknown>)['request'];
  const response = (probe as Record<string, unknown>)['response'];
  if (typeof request !== 'object' || request === null || typeof response !== 'object' || response === null) return [];
  const requestRecord = request as Record<string, unknown>;
  const responseRecord = response as Record<string, unknown>;
  if (typeof requestRecord['query'] !== 'string') return [];

  const body = Object.fromEntries(
    Object.entries(responseRecord).filter(([key]) => key !== 'status' && key !== 'headers'),
  );
  const call: RecordedUpstreamCall = {
    query: requestRecord['query'],
    variables: requestRecord['variables'] ?? {},
    response: {
      status: typeof responseRecord['status'] === 'number' ? responseRecord['status'] : 200,
      body,
    },
  };
  return validateRecordedUpstreamCalls([call]).length === 0 ? [call] : [];
}

function capturedUpstreamCalls(capture: Record<string, unknown>): RecordedUpstreamCall[] {
  const calls = Array.isArray(capture['upstreamCalls']) ? (capture['upstreamCalls'] as RecordedUpstreamCall[]) : [];
  return [...calls, ...capturedSchemaProbeCall(capture)];
}

export type ParityGidAliasBindings = {
  expectedToActual: Map<string, string>;
  actualToExpected: Map<string, string>;
  fixedIdentities: Set<string>;
};

export function createParityGidAliasBindings(): ParityGidAliasBindings {
  return { expectedToActual: new Map(), actualToExpected: new Map(), fixedIdentities: new Set() };
}

function bindGidAlias(expected: string, actual: string, bindings: ParityGidAliasBindings): void {
  if (shopifyGidType(expected) !== shopifyGidType(actual)) return;
  if (expected === actual) {
    if (!bindings.expectedToActual.has(expected) && !bindings.actualToExpected.has(actual)) {
      bindings.fixedIdentities.add(expected);
    }
    return;
  }
  if (bindings.fixedIdentities.has(expected) || bindings.fixedIdentities.has(actual)) return;
  const boundActual = bindings.expectedToActual.get(expected);
  const boundExpected = bindings.actualToExpected.get(actual);
  if (
    (boundActual !== undefined && boundActual !== actual) ||
    (boundExpected !== undefined && boundExpected !== expected)
  ) {
    return;
  }
  bindings.expectedToActual.set(expected, actual);
  bindings.actualToExpected.set(actual, expected);
}

export function bindCorrespondingGidAliases(
  expected: unknown,
  actual: unknown,
  bindings: ParityGidAliasBindings,
): void {
  if (typeof expected === 'string' && typeof actual === 'string') {
    if (shopifyGidType(expected) !== undefined && shopifyGidType(actual) !== undefined) {
      bindGidAlias(expected, actual, bindings);
    }
    return;
  }
  if (Array.isArray(expected) && Array.isArray(actual)) {
    const count = Math.min(expected.length, actual.length);
    for (let index = 0; index < count; index += 1) {
      bindCorrespondingGidAliases(expected[index], actual[index], bindings);
    }
    return;
  }
  if (!isPlainObject(expected) || !isPlainObject(actual)) return;
  for (const key of Object.keys(expected)) {
    if (Object.hasOwn(actual, key)) bindCorrespondingGidAliases(expected[key], actual[key], bindings);
  }
}

export function rewriteBoundGidAliases(value: unknown, bindings: ParityGidAliasBindings): unknown {
  if (typeof value === 'string') return bindings.expectedToActual.get(value) ?? value;
  if (Array.isArray(value)) return value.map((entry) => rewriteBoundGidAliases(entry, bindings));
  if (!isPlainObject(value)) return value;
  return Object.fromEntries(
    Object.entries(value).map(([key, entry]) => [key, rewriteBoundGidAliases(entry, bindings)]),
  );
}

function rewriteBoundGidAliasesInString(value: string, bindings: ParityGidAliasBindings): string {
  let rewritten = value;
  const aliases = [...bindings.expectedToActual.entries()].sort(([left], [right]) => right.length - left.length);
  for (const [expected, actual] of aliases) rewritten = rewritten.replaceAll(expected, actual);
  return rewritten;
}

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const paritySpecRoot = path.join(repoRoot, 'config', 'parity-specs');
const defaultAdminApiVersion = DEFAULT_ADMIN_API_VERSION;
const executableAdminApiVersions = new Set(EXECUTABLE_ADMIN_API_VERSIONS);
const defaultReadMode: ReadMode = 'live-hybrid';

function log(message: string): void {
  process.stdout.write(`${message}\n`);
}

function logError(message: string): void {
  process.stderr.write(`${message}\n`);
}

export function parseArgs(argv: string[]): CliArgs {
  const args: CliArgs = {
    all: false,
    debug: false,
    dryRun: false,
    scenarioIds: [],
    specPaths: [],
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index] ?? '';
    if (arg === '--') continue;
    if (arg === '--all') args.all = true;
    else if (arg === '--debug') args.debug = true;
    else if (arg === '--dry-run') args.dryRun = true;
    else if (arg === '--output-json') {
      const next = argv[index + 1];
      if (!next || next.startsWith('-')) throw new Error('--output-json requires a path argument');
      args.outputJsonPath = next;
      index += 1;
    } else if (arg === '--spec') {
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

export function scenarioClockFromCapture(capture: Record<string, unknown>): string | undefined {
  const captureEpochs = new Set<number>();
  let hasLifecycleTimestamp = false;
  const pending: unknown[] = [capture];
  while (pending.length > 0) {
    const candidate = pending.pop();
    if (typeof candidate === 'string') {
      if (candidate.startsWith('gid://shopify/')) continue;
      for (const match of candidate.matchAll(/(?:^|\D)(\d{13})(?!\d)/gu)) {
        const epochMilliseconds = Number(match[1]);
        const year = new Date(epochMilliseconds).getUTCFullYear();
        if (year >= 2020 && year <= 2100) captureEpochs.add(epochMilliseconds);
      }
      continue;
    }
    if (Array.isArray(candidate)) {
      pending.push(...candidate);
      continue;
    }
    if (!isPlainObject(candidate)) continue;
    for (const [key, value] of Object.entries(candidate)) {
      if (key === 'runId' && (typeof value === 'string' || typeof value === 'number')) {
        const raw = String(value);
        if (/^\d{13}$/u.test(raw)) {
          const epochMilliseconds = Number(raw);
          const year = new Date(epochMilliseconds).getUTCFullYear();
          if (year >= 2020 && year <= 2100) captureEpochs.add(epochMilliseconds);
        }
      }
      if ((key === 'startsAt' || key === 'endsAt' || key === 'reserveInventoryUntil') && isIsoTimestamp(value)) {
        hasLifecycleTimestamp = true;
      }
      pending.push(value);
    }
  }
  if (!hasLifecycleTimestamp || captureEpochs.size !== 1) return undefined;
  return new Date([...captureEpochs][0] as number).toISOString();
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
  for (const specPath of args.specPaths)
    specPaths.push(path.isAbsolute(specPath) ? specPath : path.resolve(repoRoot, specPath));
  return specPaths;
}

function tokenizeJsonPathParts(jsonPath: string, allowWildcards: boolean): string[] {
  if (!jsonPath.startsWith('$')) throw new Error(`Unsupported JSONPath (must start with $): ${jsonPath}`);
  const parts: string[] = [];
  const pattern = /\.([^.[\]]+)|\[(\d+)\]|\[\*\]/uy;
  let offset = 1;
  while (offset < jsonPath.length) {
    pattern.lastIndex = offset;
    const match = pattern.exec(jsonPath);
    if (!match) {
      throw new Error(`Unsupported JSONPath segment in ${jsonPath}: ${jsonPath.slice(offset)}`);
    }
    if (match[0] === '[*]') {
      if (!allowWildcards) throw new Error(`Unsupported JSONPath wildcard segment: ${jsonPath}`);
      parts.push('*');
    } else {
      parts.push(match[1] ?? match[2] ?? '');
    }
    offset = pattern.lastIndex;
  }
  return parts;
}

function tokenizeJsonPath(jsonPath: string): string[] {
  return tokenizeJsonPathParts(jsonPath, false);
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

function tokenizeJsonPathWithWildcards(jsonPath: string): string[] {
  return tokenizeJsonPathParts(jsonPath, true);
}

function isArrayIndex(part: string): boolean {
  return /^\d+$/u.test(part);
}

function projectPathParts(value: unknown, parts: string[]): unknown {
  if (parts.length === 0) return value;
  const [head, ...rest] = parts;
  if (head === undefined) return value;
  if (head === '*') {
    if (!Array.isArray(value)) return undefined;
    return value.map((entry) => projectPathParts(entry, rest));
  }
  const child =
    Array.isArray(value) && isArrayIndex(head)
      ? value[Number(head)]
      : typeof value === 'object' && value !== null
        ? (value as Record<string, unknown>)[head]
        : undefined;
  const projectedChild = projectPathParts(child, rest);
  if (isArrayIndex(head)) {
    const out: unknown[] = [];
    out[Number(head)] = projectedChild;
    return out;
  }
  return { [head]: projectedChild };
}

function mergeProjectedPath(existing: unknown, incoming: unknown): unknown {
  if (existing === undefined) return incoming;
  if (Array.isArray(existing) && Array.isArray(incoming)) {
    const out = existing.slice();
    for (let index = 0; index < incoming.length; index += 1) {
      if (Object.hasOwn(incoming, index)) out[index] = mergeProjectedPath(out[index], incoming[index]);
    }
    return out;
  }
  if (isPlainObject(existing) && isPlainObject(incoming)) {
    const out: Record<string, unknown> = { ...existing };
    for (const [key, value] of Object.entries(incoming)) out[key] = mergeProjectedPath(out[key], value);
    return out;
  }
  return incoming;
}

function projectPath(value: unknown, jsonPath: string): unknown {
  return projectPathParts(value, tokenizeJsonPathWithWildcards(jsonPath));
}

export function selectPaths(value: unknown, paths: string[] | undefined): unknown {
  if (!paths || paths.length === 0) return value;
  let out: unknown = undefined;
  for (const jsonPath of paths) out = mergeProjectedPath(out, projectPath(value, jsonPath));
  return out;
}

function deepClone<T>(value: T): T {
  return value === undefined ? value : (JSON.parse(JSON.stringify(value)) as T);
}

function deletePathParts(cursor: unknown, parts: string[]): void {
  if (parts.length === 0 || cursor === undefined || cursor === null) return;
  const [head, ...rest] = parts;
  // parts.length !== 0 is guaranteed above, so head is always defined; this guard
  // only narrows `string | undefined` to `string` for the index accesses below.
  if (head === undefined) return;
  if (head === '*') {
    // Wildcard array segment: recurse into every element.
    if (Array.isArray(cursor)) for (const item of cursor) deletePathParts(item, rest);
    return;
  }
  if (rest.length === 0) {
    if (Array.isArray(cursor)) cursor.splice(Number(head), 1);
    else if (typeof cursor === 'object' && cursor !== null) delete (cursor as Record<string, unknown>)[head];
    return;
  }
  const next = Array.isArray(cursor) ? cursor[Number(head)] : (cursor as Record<string, unknown> | undefined)?.[head];
  deletePathParts(next, rest);
}

function deletePath(root: unknown, jsonPath: string): unknown {
  const copy = deepClone(root);
  const parts = tokenizeJsonPathWithWildcards(jsonPath);
  if (parts.length === 0) return undefined;
  deletePathParts(copy, parts);
  return copy;
}

function applyExcludedPaths(value: unknown, paths: string[] | undefined): unknown {
  let out = value;
  for (const jsonPath of paths ?? []) out = deletePath(out, jsonPath);
  return out;
}

function resourceIdTail(value: string): string {
  const pathPart = value.split('?')[0] ?? value;
  return pathPart.split('/').pop() ?? value;
}

function applySpecialVariableTransforms(value: unknown, object: Record<string, unknown>): unknown {
  let out = value;
  if (object['resourceIdTail'] === true) {
    if (typeof out !== 'string') throw new Error('resourceIdTail transform requires a string value');
    out = resourceIdTail(out);
  }
  if (typeof object['prefix'] === 'string' || typeof object['suffix'] === 'string') {
    if (!['string', 'number', 'boolean'].includes(typeof out)) {
      throw new Error('prefix/suffix transform requires a scalar value');
    }
    const prefix = typeof object['prefix'] === 'string' ? object['prefix'] : '';
    const suffix = typeof object['suffix'] === 'string' ? object['suffix'] : '';
    out = `${prefix}${String(out)}${suffix}`;
  }
  return out;
}

function resolveSpecialVariables(
  value: unknown,
  capture: unknown,
  primaryResponse: ProxyResponse | null,
  previousResponse: ProxyResponse | null,
  namedResponses: Map<string, ProxyResponse>,
): unknown {
  if (Array.isArray(value))
    return value.map((entry) =>
      resolveSpecialVariables(entry, capture, primaryResponse, previousResponse, namedResponses),
    );
  if (typeof value === 'object' && value !== null) {
    const object = value as Record<string, unknown>;
    if (typeof object['fromPrimaryProxyPath'] === 'string') {
      if (primaryResponse === null) throw new Error('fromPrimaryProxyPath used before primary proxy response exists');
      return applySpecialVariableTransforms(getPath(primaryResponse.body, object['fromPrimaryProxyPath']), object);
    }
    if (typeof object['fromPreviousProxyPath'] === 'string') {
      if (previousResponse === null)
        throw new Error('fromPreviousProxyPath used before a previous proxy response exists');
      return applySpecialVariableTransforms(getPath(previousResponse.body, object['fromPreviousProxyPath']), object);
    }
    if (typeof object['fromCapturePath'] === 'string')
      return applySpecialVariableTransforms(getPath(capture, object['fromCapturePath']), object);
    if (typeof object['fromProxyResponse'] === 'string' && typeof object['path'] === 'string') {
      const response = namedResponses.get(object['fromProxyResponse']);
      if (!response) throw new Error(`fromProxyResponse references unknown target: ${object['fromProxyResponse']}`);
      return applySpecialVariableTransforms(getPath(response.body, object['path']), object);
    }
    return Object.fromEntries(
      Object.entries(object).map(([key, entry]) => [
        key,
        resolveSpecialVariables(entry, capture, primaryResponse, previousResponse, namedResponses),
      ]),
    );
  }
  return value;
}

function proxyGraphqlPath(request: ProxyRequestSpec | undefined, defaultApiVersion = defaultAdminApiVersion): string {
  const apiVersion = request?.apiVersion ?? defaultApiVersion;
  if (request?.apiSurface === 'storefront') {
    return `/api/${apiVersion}/graphql.json`;
  }
  return `/admin/api/${apiVersion}/graphql.json`;
}

async function loadRequest(
  request: ProxyRequestSpec | undefined,
  capture: unknown,
  primaryResponse: ProxyResponse | null,
  previousResponse: ProxyResponse | null,
  namedResponses: Map<string, ProxyResponse>,
  defaultApiVersion: string,
): Promise<{
  query: string;
  operationName?: string | null;
  variables: Record<string, unknown>;
  headers: Record<string, string>;
  path: string;
  apiSurface: ApiSurface;
} | null> {
  if (!request || (!request.documentPath && !request.documentCapturePath)) return null;
  let query: string;
  if (request.documentCapturePath) {
    const document = getPath(capture, request.documentCapturePath);
    if (typeof document !== 'string')
      throw new Error(`Spec references missing captured document: ${request.documentCapturePath}`);
    query = document;
  } else {
    const documentPath = path.resolve(repoRoot, request.documentPath ?? '');
    if (!existsSync(documentPath)) throw new Error(`Spec references missing document: ${request.documentPath ?? ''}`);
    query = await readFile(documentPath, 'utf8');
  }

  let variables: Record<string, unknown> = {};
  if (request.variablesCapturePath)
    variables = (getPath(capture, request.variablesCapturePath) ?? {}) as Record<string, unknown>;
  else if (request.variablesPath) variables = await readJsonFile(path.resolve(repoRoot, request.variablesPath));
  else if (request.variables) variables = request.variables;

  let operationName = request.operationName;
  if (request.operationNameCapturePath) {
    const capturedOperationName = getPath(capture, request.operationNameCapturePath);
    if (
      capturedOperationName !== null &&
      capturedOperationName !== undefined &&
      typeof capturedOperationName !== 'string'
    ) {
      throw new Error(`Spec references non-string captured operationName: ${request.operationNameCapturePath}`);
    }
    operationName = capturedOperationName ?? null;
  }

  variables = resolveSpecialVariables(variables, capture, primaryResponse, previousResponse, namedResponses) as Record<
    string,
    unknown
  >;
  const apiSurface = request.apiSurface ?? 'admin';
  const headers = { ...request.headers };
  if (apiSurface === 'storefront') {
    const hasStorefrontToken = Object.keys(headers).some((name) => /storefront.*token/iu.test(name));
    if (!hasStorefrontToken) {
      headers['X-Shopify-Storefront-Access-Token'] = '<redacted:storefront-access-token>';
    }
  }
  const loadedRequest: {
    query: string;
    operationName?: string | null;
    variables: Record<string, unknown>;
    headers: Record<string, string>;
    path: string;
    apiSurface: ApiSurface;
  } = {
    query,
    variables,
    headers,
    path: proxyGraphqlPath(request, defaultApiVersion),
    apiSurface,
  };
  if (operationName !== undefined) loadedRequest.operationName = operationName;
  return loadedRequest;
}

type LoadedProxyRequest = {
  query: string;
  operationName?: string | null;
  variables: Record<string, unknown>;
  headers: Record<string, string>;
  path: string;
  apiSurface: ApiSurface;
};

export function defaultApiVersionForCapture(capturePath: string, capture: Record<string, unknown>): string {
  const declared = capture['apiVersion'];
  if (typeof declared === 'string') {
    if (executableAdminApiVersions.has(declared)) return declared;
    throw new Error(
      `Capture declares Admin API ${declared}, but the proxy has executable schemas only for ${EXECUTABLE_ADMIN_API_VERSIONS.join(', ')}`,
    );
  }
  const versionSegment = capturePath.split(/[\\/]/u).find((segment) => /^\d{4}-(?:01|04|07|10)$/u.test(segment));
  if (versionSegment && !executableAdminApiVersions.has(versionSegment)) {
    throw new Error(
      `Capture path uses Admin API ${versionSegment}, but the proxy has executable schemas only for ${EXECUTABLE_ADMIN_API_VERSIONS.join(', ')}`,
    );
  }
  const pathVersion = versionSegment && executableAdminApiVersions.has(versionSegment) ? versionSegment : undefined;
  return pathVersion ?? defaultAdminApiVersion;
}

type CassetteServer = {
  origin: string;
  setCalls: (calls: RecordedUpstreamCall[]) => void;
  setFallbackResponse: (
    response: ProxyResponse | null,
    request?: LoadedProxyRequest | null,
    preferFallback?: boolean,
  ) => void;
  consumed: () => number;
  expected: () => number;
  close: () => Promise<void>;
};

async function startCassetteServer(): Promise<CassetteServer> {
  let calls: RecordedUpstreamCall[] = [];
  let fallbackResponse: { response: ProxyResponse; call: RecordedUpstreamCall } | null = null;
  let preferFallback = false;
  let fallbackCount = 0;
  const consumedCalls = new Set<number>();
  const server = createServer((request: IncomingMessage, response: ServerResponse) => {
    let body = '';
    request.setEncoding('utf8');
    request.on('data', (chunk) => (body += chunk));
    request.on('end', () => {
      if (process.env['PARITY_LOG_UPSTREAM']) {
        try {
          appendFileSync(process.env['PARITY_LOG_UPSTREAM'], `${body}\n@@@PARITY_UPSTREAM_SEP@@@\n`);
        } catch {
          /* diagnostic only */
        }
      }
      const requestPath = request.url ? new URL(request.url, 'http://127.0.0.1').pathname : '/';
      const outgoingRequest: OutgoingGraphqlRequest = {
        method: request.method ?? 'GET',
        path: requestPath,
        body,
      };
      const inferredApiSurface = apiSurfaceFromGraphqlPath(requestPath);
      if (inferredApiSurface !== null) outgoingRequest.apiSurface = inferredApiSurface;
      if (
        preferFallback &&
        fallbackResponse !== null &&
        recordedCallMatchesRequest(fallbackResponse.call, outgoingRequest)
      ) {
        fallbackCount += 1;
        response.statusCode = fallbackResponse.response.status;
        response.setHeader('content-type', 'application/json');
        response.end(JSON.stringify(fallbackResponse.response.body));
        return;
      }
      const matchedIndex = calls.findIndex(
        (call, callIndex) => !consumedCalls.has(callIndex) && recordedCallMatchesRequest(call, outgoingRequest),
      );
      if (matchedIndex >= 0) {
        const call = calls[matchedIndex];
        consumedCalls.add(matchedIndex);
        response.statusCode = call?.response?.status ?? 200;
        response.setHeader('content-type', 'application/json');
        // Support two response shapes:
        //   { body: <graphql-payload> } — the typed RecordedUpstreamCall shape
        //   { data: ..., errors: ... }  — raw GraphQL payload stored directly as response
        const responseBody =
          call?.response?.body !== undefined
            ? call.response.body
            : (call?.response as Record<string, unknown> | undefined)?.['data'] !== undefined
              ? call?.response
              : {};
        response.end(JSON.stringify(responseBody));
        return;
      }
      if (fallbackResponse !== null && recordedCallMatchesRequest(fallbackResponse.call, outgoingRequest)) {
        fallbackCount += 1;
        response.statusCode = fallbackResponse.response.status;
        response.setHeader('content-type', 'application/json');
        response.end(JSON.stringify(fallbackResponse.response.body));
        return;
      }
      response.statusCode = 500;
      response.setHeader('content-type', 'application/json');
      response.end(
        JSON.stringify({ errors: [{ message: formatRecordedCallMismatch(outgoingRequest, calls, consumedCalls) }] }),
      );
    });
  });
  await new Promise<void>((resolveListen) => server.listen(0, '127.0.0.1', resolveListen));
  const address = server.address();
  if (address === null || typeof address === 'string') throw new Error('Failed to start cassette server');
  return {
    origin: `http://127.0.0.1:${address.port}`,
    setCalls: (nextCalls: RecordedUpstreamCall[]) => {
      calls = nextCalls;
      fallbackResponse = null;
      preferFallback = false;
      fallbackCount = 0;
      consumedCalls.clear();
    },
    setFallbackResponse: (
      response: ProxyResponse | null,
      request?: LoadedProxyRequest | null,
      nextPreferFallback = false,
    ) => {
      preferFallback = nextPreferFallback;
      fallbackResponse =
        response && request
          ? {
              response,
              call: {
                method: 'POST',
                path: request.path,
                apiSurface: request.apiSurface,
                query: request.query,
                variables: request.variables,
              },
            }
          : null;
    },
    consumed: () => consumedCalls.size,
    expected: () => calls.length + fallbackCount,
    close: async () =>
      await new Promise<void>((resolveClose, reject) =>
        server.close((error) => (error ? reject(error) : resolveClose())),
      ),
  };
}

async function sendProxyRequest(proxy: DraftProxy, request: LoadedProxyRequest): Promise<ProxyResponse> {
  const body: Record<string, unknown> = { query: request.query, variables: request.variables };
  if ('operationName' in request) body['operationName'] = request.operationName ?? null;
  return await proxy.processRequest({
    method: 'POST',
    path: request.path,
    headers: { 'content-type': 'application/json', ...request.headers },
    body,
  });
}

function localProxyPath(requestPath: unknown, targetName: string): string {
  if (typeof requestPath !== 'string') {
    throw new Error(`${targetName}: proxyHttpRequest path did not resolve to a string`);
  }
  if (!requestPath.startsWith('http://') && !requestPath.startsWith('https://')) return requestPath;
  return new URL(requestPath).pathname;
}

async function sendProxyHttpRequest(
  proxy: DraftProxy,
  targetName: string,
  request: ProxyHttpRequestSpec,
  capture: unknown,
  primaryResponse: ProxyResponse | null,
  previousResponse: ProxyResponse | null,
  namedResponses: Map<string, ProxyResponse>,
): Promise<ProxyResponse> {
  const resolvedPath = resolveSpecialVariables(
    request.path,
    capture,
    primaryResponse,
    previousResponse,
    namedResponses,
  );
  const resolvedBody = resolveSpecialVariables(
    request.body ?? '',
    capture,
    primaryResponse,
    previousResponse,
    namedResponses,
  );
  const proxyRequest: DraftProxyRequest = {
    method: request.method ?? 'GET',
    path: localProxyPath(resolvedPath, targetName),
    body: resolvedBody,
  };
  if (request.headers !== undefined) proxyRequest.headers = request.headers;
  return await proxy.processRequest(proxyRequest);
}

function localUploadPath(uploadPath: unknown, targetName: string): string {
  if (typeof uploadPath !== 'string') throw new Error(`${targetName}: proxyUpload path did not resolve to a string`);
  if (!uploadPath.startsWith('http://') && !uploadPath.startsWith('https://')) return uploadPath;
  const parsed = new URL(uploadPath);
  if (parsed.pathname !== '/') return parsed.pathname;
  return `/staged-uploads/${encodeURIComponent(targetName)}/upload.jsonl`;
}

async function sendProxyUpload(
  proxy: DraftProxy,
  targetName: string,
  upload: ProxyUploadSpec,
  capture: unknown,
  primaryResponse: ProxyResponse | null,
  previousResponse: ProxyResponse | null,
  namedResponses: Map<string, ProxyResponse>,
): Promise<ProxyResponse> {
  const resolvedPath = resolveSpecialVariables(upload.path, capture, primaryResponse, previousResponse, namedResponses);
  const resolvedBody = resolveSpecialVariables(
    upload.body ?? '',
    capture,
    primaryResponse,
    previousResponse,
    namedResponses,
  );
  const request: { method: string; path: string; headers?: Record<string, string>; body: unknown } = {
    method: upload.method ?? 'POST',
    path: localUploadPath(resolvedPath, targetName),
    body: resolvedBody,
  };
  if (upload.headers !== undefined) request.headers = upload.headers;
  const response = await proxy.processRequest(request);
  if (response.status >= 400) throw new Error(`${targetName}: proxyUpload failed with status ${response.status}`);
  return response;
}

export function parseJsonlRecordsForParity(value: unknown): unknown {
  if (Array.isArray(value)) return value;
  if (typeof value !== 'string') return value;
  return value
    .split('\n')
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line) as unknown);
}

function normalizeForTarget(value: unknown, target: ComparisonTarget): unknown {
  const comparable = target.jsonlRecords ? parseJsonlRecordsForParity(value) : value;
  return applyExcludedPaths(selectPaths(comparable, target.selectedPaths), target.excludedPaths);
}

export function captureResponseForTarget(capture: unknown, target: ComparisonTarget): ProxyResponse | null {
  for (const payloadPrefix of ['.result.body', '.response.body']) {
    const payloadIndex = target.capturePath.indexOf(payloadPrefix);
    if (payloadIndex === -1) continue;
    const payloadPath = target.capturePath.slice(0, payloadIndex + payloadPrefix.length);
    const payload = getPath(capture, payloadPath);
    if (payload === undefined) return null;
    const statusPath = `${target.capturePath.slice(0, payloadIndex)}${payloadPrefix.replace('.body', '.status')}`;
    const status = getPath(capture, statusPath);
    return { status: typeof status === 'number' ? status : 200, body: payload };
  }
  for (const payloadPrefix of ['.result.payload', '.response.payload']) {
    const payloadIndex = target.capturePath.indexOf(payloadPrefix);
    if (payloadIndex === -1) continue;
    const payloadPath = target.capturePath.slice(0, payloadIndex + payloadPrefix.length);
    const payload = getPath(capture, payloadPath);
    if (payload === undefined) return null;
    const statusPath = `${target.capturePath.slice(0, payloadIndex)}${payloadPrefix.replace('.payload', '.status')}`;
    const status = getPath(capture, statusPath);
    return { status: typeof status === 'number' ? status : 200, body: payload };
  }
  for (const responsePrefix of ['.result', '.response']) {
    const responseIndex = target.capturePath.indexOf(responsePrefix);
    if (responseIndex === -1) continue;
    const responsePath = target.capturePath.slice(0, responseIndex + responsePrefix.length);
    const response = getPath(capture, responsePath);
    if (response === undefined) return null;
    const status = getPath(capture, `${responsePath}.status`);
    return { status: typeof status === 'number' ? status : 200, body: response };
  }
  if (target.capturePath.endsWith('.data')) {
    const data = getPath(capture, target.capturePath);
    if (data !== undefined) return { status: 200, body: { data } };
  }
  return null;
}

function normalizedCapturePayload(value: unknown): unknown {
  if (typeof value !== 'object' || value === null) return null;
  const object = value as Record<string, unknown>;
  if (typeof object['payload'] === 'object' && object['payload'] !== null) return object['payload'];
  if (typeof object['body'] === 'object' && object['body'] !== null) return object['body'];
  return object;
}

function capturedProxyResponse(value: unknown): ProxyResponse | null {
  const body = normalizedCapturePayload(value);
  if (body === null) return null;
  const status =
    typeof value === 'object' && value !== null && typeof (value as Record<string, unknown>)['status'] === 'number'
      ? ((value as Record<string, unknown>)['status'] as number)
      : 200;
  return { status, body };
}

function capturedRequestMatches(entry: Record<string, unknown>, request: LoadedProxyRequest): boolean {
  return (
    typeof entry['query'] === 'string' &&
    (entry['query'] as string).trimEnd() === request.query.trimEnd() &&
    stableJson(entry['variables'] ?? {}) === stableJson(request.variables ?? {}) &&
    (!('operationName' in entry) || (entry['operationName'] ?? null) === (request.operationName ?? null))
  );
}

export function captureResponseForRequest(capture: unknown, request: LoadedProxyRequest): ProxyResponse | null {
  const pending: unknown[] = [capture];
  while (pending.length > 0) {
    const candidate = pending.pop();
    if (Array.isArray(candidate)) {
      pending.push(...candidate);
      continue;
    }
    if (typeof candidate !== 'object' || candidate === null) continue;
    const entry = candidate as Record<string, unknown>;
    if (capturedRequestMatches(entry, request)) {
      const response = capturedProxyResponse(entry['response'] ?? entry['result']);
      if (response !== null) return response;
    }
    const nestedRequest = entry['request'];
    if (typeof nestedRequest === 'object' && nestedRequest !== null && !Array.isArray(nestedRequest)) {
      if (capturedRequestMatches(nestedRequest as Record<string, unknown>, request)) {
        const response = capturedProxyResponse(entry['response'] ?? entry['result']);
        if (response !== null) return response;
      }
    }
    for (const value of Object.values(entry)) pending.push(value);
  }
  return null;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isIsoTimestamp(value: unknown): boolean {
  if (typeof value !== 'string') return false;
  const match = /^(\d{4})-(\d{2})-(\d{2})T([01]\d|2[0-3]):([0-5]\d):([0-5]\d)(?:\.\d+)?Z$/u.exec(value);
  if (!match) return false;

  const timestamp = Date.parse(value);
  if (Number.isNaN(timestamp)) return false;

  const parsed = new Date(timestamp);
  return (
    parsed.getUTCFullYear() === Number(match[1]) &&
    parsed.getUTCMonth() + 1 === Number(match[2]) &&
    parsed.getUTCDate() === Number(match[3]) &&
    parsed.getUTCHours() === Number(match[4]) &&
    parsed.getUTCMinutes() === Number(match[5]) &&
    parsed.getUTCSeconds() === Number(match[6])
  );
}

function isJsonlString(value: unknown): boolean {
  if (typeof value !== 'string') return false;
  const lines = value.split('\n').filter((line) => line.length > 0);
  return lines.every((line) => {
    try {
      const parsed = JSON.parse(line) as unknown;
      return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed);
    } catch {
      return false;
    }
  });
}

function shopifyGidType(value: unknown): string | undefined {
  if (typeof value !== 'string') return undefined;
  return /^gid:\/\/shopify\/([A-Za-z][A-Za-z0-9]*)\//u.exec(value)?.[1];
}

function matchesRule(value: unknown, rule: ExpectedDifference, gidAliases: ParityGidAliasBindings): boolean {
  if (rule.ignore) return true;
  const matcher = rule.matcher ?? '';
  if (matcher === 'null') return value === null;
  if (matcher === 'any-string') return typeof value === 'string';
  if (matcher === 'non-empty-string') return typeof value === 'string' && value.length > 0;
  if (matcher === 'any-number') return typeof value === 'number';
  if (matcher === 'iso-timestamp') return isIsoTimestamp(value);
  if (matcher === 'jsonl-string') return isJsonlString(value);
  if (matcher === 'storefront-access-token') return typeof value === 'string' && value.length > 0;
  const gidMatch = /^shopify-gid:([A-Za-z][A-Za-z0-9]*)$/u.exec(matcher);
  if (gidMatch) return typeof value === 'string' && value.startsWith(`gid://shopify/${gidMatch[1]}/`);
  if (matcher.startsWith('exact-string:')) {
    const expected = matcher.slice('exact-string:'.length);
    const expectedType = shopifyGidType(expected);
    const actualType = shopifyGidType(value);
    if (expectedType === undefined || actualType === undefined) return value === expected;
    if (expectedType !== actualType || typeof value !== 'string') return false;

    // Exact proxy-local GIDs are scenario aliases: their old numeric tails are
    // not stable under the instance-wide broker, but equality and distinctness
    // across targets still prove relationship preservation.
    const boundActual = gidAliases.expectedToActual.get(expected);
    if (boundActual !== undefined) return value === boundActual;
    const boundExpected = gidAliases.actualToExpected.get(value);
    if (boundExpected !== undefined && boundExpected !== expected) return false;
    gidAliases.expectedToActual.set(expected, value);
    gidAliases.actualToExpected.set(value, expected);
    return true;
  }
  if (matcher.startsWith('regex:'))
    return typeof value === 'string' && new RegExp(matcher.slice('regex:'.length), 'u').test(value);
  if (matcher.startsWith('shop-policy-url-base:'))
    return typeof value === 'string' && value.startsWith(matcher.slice('shop-policy-url-base:'.length));
  return false;
}

function ruleMatchesPath(rulePath: string, actualPath: string): boolean {
  if (rulePath === actualPath) return true;
  const wildcard = '\0ARRAY_INDEX_WILDCARD\0';
  const pattern = `^${rulePath
    .replace(/\[\*\]/gu, wildcard)
    .replace(/[\\^$*+?.()|[\]{}]/gu, '\\$&')
    .replaceAll(wildcard, String.raw`\[\d+\]`)}$`;
  return new RegExp(pattern, 'u').test(actualPath);
}

export function diffValues(
  capture: unknown,
  proxy: unknown,
  rules: ExpectedDifference[],
  basePath = '$',
  gidAliases = createParityGidAliasBindings(),
): string[] {
  if (
    typeof capture === 'string' &&
    typeof proxy === 'string' &&
    rewriteBoundGidAliasesInString(capture, gidAliases) === proxy
  ) {
    return [];
  }
  if (
    typeof capture === 'string' &&
    typeof proxy === 'string' &&
    gidAliases.expectedToActual.get(capture) === proxy &&
    gidAliases.actualToExpected.get(proxy) === capture
  ) {
    return [];
  }
  const rule = rules.find((candidate) => ruleMatchesPath(candidate.path, basePath));
  if (rule && matchesRule(proxy, rule, gidAliases)) return [];
  if (Object.is(capture, proxy)) return [];
  if (Array.isArray(capture) && Array.isArray(proxy)) {
    const errors: string[] = [];
    const max = Math.max(capture.length, proxy.length);
    for (let index = 0; index < max; index += 1)
      errors.push(...diffValues(capture[index], proxy[index], rules, `${basePath}[${index}]`, gidAliases));
    return errors;
  }
  if (isPlainObject(capture) && isPlainObject(proxy)) {
    const errors: string[] = [];
    const keys = new Set([...Object.keys(capture), ...Object.keys(proxy)]);
    for (const key of [...keys].sort())
      errors.push(...diffValues(capture[key], proxy[key], rules, `${basePath}.${key}`, gidAliases));
    return errors;
  }
  return [`${basePath}: expected ${JSON.stringify(capture)} got ${JSON.stringify(proxy)}`];
}

async function runSpec(
  specPath: string,
  debug: boolean,
  proxy: DraftProxy,
  cassette: CassetteServer,
  cleanState: DraftProxyStateDump,
): Promise<string[]> {
  const relativeSpecPath = path.relative(repoRoot, specPath);
  const spec = await readJsonFile<ParitySpec>(specPath);
  const capturePath = spec.liveCaptureFiles?.[0];
  if (!capturePath) return [`${relativeSpecPath}: spec has no liveCaptureFiles[0]`];
  const capture = await readJsonFile<Record<string, unknown>>(path.resolve(repoRoot, capturePath));
  const defaultApiVersion = defaultApiVersionForCapture(capturePath, capture);
  const upstreamCalls = capturedUpstreamCalls(capture);
  cassette.setCalls(upstreamCalls);
  proxy.restoreState(cleanState);
  await proxy.processRequest({ method: 'POST', path: '/__meta/reset' });
  const failures: string[] = [];
  const gidAliases = createParityGidAliasBindings();
  let primaryResponse: ProxyResponse | null = null;
  let previousResponse: ProxyResponse | null = null;
  const namedResponses = new Map<string, ProxyResponse>();
  try {
    for (const setup of spec.proxySetups ?? []) {
      const setupRequest = await loadRequest(
        setup.proxyRequest,
        capture,
        null,
        previousResponse,
        namedResponses,
        defaultApiVersion,
      );
      if (setupRequest === null) throw new Error(`${setup.name}: proxy setup did not resolve to a request`);
      setupRequest.variables = rewriteBoundGidAliases(setupRequest.variables, gidAliases) as Record<string, unknown>;
      const capturedSetupSource = getPath(capture, setup.captureResponsePath);
      const capturedSetupProxyResponse = capturedProxyResponse(capturedSetupSource);
      if (capturedSetupProxyResponse === null) {
        throw new Error(`${setup.name}: captureResponsePath did not resolve to a captured GraphQL response`);
      }
      // A captured setup query may be an ordinary passthrough read. Replaying its
      // exact recorded request/response pair as the temporary fallback lets that
      // read enter the proxy through the public GraphQL route without duplicating
      // it into `upstreamCalls` or importing its result into private state.
      cassette.setFallbackResponse(capturedSetupProxyResponse, setupRequest, true);
      const setupResponse = await sendProxyRequest(proxy, setupRequest);
      const capturedSetupResponse = capturedSetupProxyResponse.body;
      bindCorrespondingGidAliases(capturedSetupResponse, setupResponse.body, gidAliases);
      namedResponses.set(setup.name, setupResponse);
      previousResponse = setupResponse;
      if (debug) {
        log(
          `[parity-debug] ${relativeSpecPath} [setup:${setup.name}] proxy response ${JSON.stringify(setupResponse.body).slice(0, 1000)}`,
        );
      }
    }
    const primaryRequest = await loadRequest(spec.proxyRequest, capture, null, null, namedResponses, defaultApiVersion);
    if (primaryRequest !== null) {
      primaryRequest.variables = rewriteBoundGidAliases(primaryRequest.variables, gidAliases) as Record<
        string,
        unknown
      >;
      const primaryFallbackTarget =
        spec.comparison?.targets?.find(
          (target) =>
            !target.proxyRequest &&
            !target.proxyUpload &&
            !target.proxyStatePath &&
            !target.proxyLogPath &&
            captureResponseForTarget(capture, target) !== null,
        ) ?? spec.comparison?.targets?.find((target) => captureResponseForTarget(capture, target) !== null);
      const primaryFallbackResponse =
        captureResponseForRequest(capture, primaryRequest) ??
        (primaryFallbackTarget ? captureResponseForTarget(capture, primaryFallbackTarget) : null);
      cassette.setFallbackResponse(primaryFallbackResponse, primaryRequest);
      primaryResponse = await sendProxyRequest(proxy, primaryRequest);
      previousResponse = primaryResponse;
      if (primaryFallbackResponse !== null) {
        bindCorrespondingGidAliases(primaryFallbackResponse.body, primaryResponse.body, gidAliases);
      }
      if (debug) {
        log(
          `[parity-debug] ${relativeSpecPath} [primary] proxy response ${JSON.stringify(primaryResponse.body).slice(0, 1000)}`,
        );
        if (gidAliases.expectedToActual.size > 0) {
          log(
            `[parity-debug] ${relativeSpecPath} [primary] gid aliases ${JSON.stringify(Object.fromEntries(gidAliases.expectedToActual))}`,
          );
        }
      }
    }
    let mainState = proxy.dumpState('1970-01-01T00:00:00.000Z');

    for (const target of spec.comparison?.targets ?? []) {
      let proxySource: unknown;
      let targetResponseForAliases: ProxyResponse | null = null;
      if (target.isolatedProxy) {
        cassette.setCalls(upstreamCalls);
        await proxy.processRequest({ method: 'POST', path: '/__meta/reset' });
        primaryResponse = null;
        previousResponse = null;
        namedResponses.clear();
        gidAliases.expectedToActual.clear();
        gidAliases.actualToExpected.clear();
        gidAliases.fixedIdentities.clear();
      } else if (target.preserveProxyState !== true) {
        proxy.restoreState(mainState);
      }
      if (target.proxyUpload) {
        const uploadResponse = await sendProxyUpload(
          proxy,
          target.name,
          target.proxyUpload,
          capture,
          primaryResponse,
          previousResponse,
          namedResponses,
        );
        previousResponse = uploadResponse;
        proxySource = getPath(capture, target.capturePath);
      } else if (target.proxyRequest) {
        const request = await loadRequest(
          target.proxyRequest,
          capture,
          primaryResponse,
          previousResponse,
          namedResponses,
          defaultApiVersion,
        );
        if (request === null) throw new Error(`${target.name}: target proxyRequest did not resolve to a request`);
        if (target.rewriteGidAliases !== false) {
          request.variables = rewriteBoundGidAliases(request.variables, gidAliases) as Record<string, unknown>;
        }
        if (debug && gidAliases.expectedToActual.size > 0) {
          log(
            `[parity-debug] ${relativeSpecPath} [${target.name}] aliased variables ${JSON.stringify(request.variables).slice(0, 1000)}`,
          );
        }
        cassette.setFallbackResponse(
          captureResponseForTarget(capture, target),
          request,
          target.preferTargetFallback === true,
        );
        const targetResponse = await sendProxyRequest(proxy, request);
        targetResponseForAliases = targetResponse;
        if (!target.isolatedProxy && target.preserveProxyState !== true) {
          mainState = proxy.dumpState('1970-01-01T00:00:00.000Z');
        }
        namedResponses.set(target.name, targetResponse);
        previousResponse = targetResponse;
        proxySource = targetResponse.body;
        if (debug)
          log(
            `[parity-debug] ${relativeSpecPath} [${target.name}] proxy response ${JSON.stringify(proxySource).slice(0, 1000)}`,
          );
      } else if (target.proxyHttpRequest) {
        const targetResponse = await sendProxyHttpRequest(
          proxy,
          target.name,
          target.proxyHttpRequest,
          capture,
          primaryResponse,
          previousResponse,
          namedResponses,
        );
        namedResponses.set(target.name, targetResponse);
        previousResponse = targetResponse;
        proxySource = targetResponse;
        if (debug)
          log(
            `[parity-debug] ${relativeSpecPath} [${target.name}] proxy HTTP response ${JSON.stringify(proxySource).slice(0, 1000)}`,
          );
      } else if (target.proxyStatePath) {
        proxySource = await proxy.getState();
      } else if (target.proxyLogPath) {
        proxySource = await proxy.getLog();
      } else {
        proxySource = primaryResponse?.body;
        if (primaryResponse) {
          namedResponses.set(target.name, primaryResponse);
          previousResponse = primaryResponse;
        }
      }
      if (targetResponseForAliases !== null) {
        const capturedTargetResponse = captureResponseForTarget(capture, target);
        if (capturedTargetResponse !== null) {
          bindCorrespondingGidAliases(capturedTargetResponse.body, targetResponseForAliases.body, gidAliases);
        }
      }
      const captureValue = normalizeForTarget(getPath(capture, target.capturePath), target);
      const proxyPath = target.proxyPath ?? target.proxyStatePath ?? target.proxyLogPath ?? '$';
      const proxyValue = normalizeForTarget(getPath(proxySource, proxyPath), target);
      const rules = [...(spec.comparison?.expectedDifferences ?? []), ...(target.expectedDifferences ?? [])];
      const diffs = diffValues(captureValue, proxyValue, rules, '$', gidAliases);
      if (diffs.length > 0) {
        failures.push(`${relativeSpecPath} [${target.name}] ${diffs.slice(0, debug ? 20 : 3).join('; ')}`);
      }
    }
    // Captured upstream calls are cassette inputs for passthrough branches, not a
    // required side effect. Rust-local handlers may satisfy the parity assertion
    // without consuming Shopify recordings.
  } catch (error) {
    failures.push(`${relativeSpecPath}: ${(error as Error).stack ?? (error as Error).message}`);
  }
  return failures;
}

function createProxyContext(
  readMode: ReadMode,
  shopifyAdminOrigin: string,
  fixedNow?: string,
  shopifyStoreDomain?: string,
): ProxyContext {
  const envName = 'SHOPIFY_DRAFT_PROXY_FIXED_NOW';
  const previousFixedNow = process.env[envName];
  if (fixedNow === undefined) delete process.env[envName];
  else process.env[envName] = fixedNow;
  let proxy: DraftProxy;
  try {
    proxy = createDraftProxy({
      readMode,
      unsupportedMutationMode: 'passthrough',
      shopifyAdminOrigin,
      ...(shopifyStoreDomain === undefined ? {} : { shopifyStoreDomain }),
      port: 0,
    });
  } finally {
    if (previousFixedNow === undefined) delete process.env[envName];
    else process.env[envName] = previousFixedNow;
  }
  return { proxy, cleanState: proxy.dumpState('1970-01-01T00:00:00.000Z') };
}

async function main(): Promise<void> {
  let args: CliArgs;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (error) {
    logError((error as Error).message);
    logError('Usage: pnpm parity <scenario-id> | --spec <path> | --all [--debug] [--dry-run] [--output-json <path>]');
    process.exit(2);
    return;
  }
  if (!args.all && args.scenarioIds.length === 0 && args.specPaths.length === 0) {
    logError('Usage: pnpm parity <scenario-id> | --spec <path> | --all [--debug] [--dry-run] [--output-json <path>]');
    process.exit(2);
    return;
  }

  const specPaths = await resolveSpecPaths(args);
  log(`[parity] ${specPaths.length} spec(s) selected`);
  if (args.dryRun) return;

  const cassette = await startCassetteServer();
  const proxyContexts = new Map<string, ProxyContext>();
  function proxyContextFor(readMode: ReadMode, shopifyStoreDomain?: string): ProxyContext {
    const key = `${readMode}\0${shopifyStoreDomain ?? ''}`;
    const existing = proxyContexts.get(key);
    if (existing) return existing;
    const context = createProxyContext(readMode, cassette.origin, undefined, shopifyStoreDomain);
    proxyContexts.set(key, context);
    return context;
  }

  const failedSpecs: Array<{ specPath: string; errors: string[] }> = [];
  const passedSpecs: string[] = [];
  try {
    for (const specPath of specPaths) {
      const spec = await readJsonFile<ParitySpec>(specPath);
      const readMode = spec.proxyConfig?.readMode ?? defaultReadMode;
      const capturePath = spec.liveCaptureFiles?.[0];
      const capture =
        capturePath === undefined
          ? undefined
          : await readJsonFile<Record<string, unknown>>(path.resolve(repoRoot, capturePath));
      const shopifyStoreDomain =
        typeof capture?.['storeDomain'] === 'string' && capture['storeDomain'].trim() !== ''
          ? capture['storeDomain']
          : undefined;
      const fixedNow = capture === undefined ? undefined : scenarioClockFromCapture(capture);
      const temporaryContext =
        fixedNow === undefined
          ? undefined
          : createProxyContext(readMode, cassette.origin, fixedNow, shopifyStoreDomain);
      const { proxy, cleanState } = temporaryContext ?? proxyContextFor(readMode, shopifyStoreDomain);
      let errors: string[];
      try {
        errors = await runSpec(specPath, args.debug, proxy, cassette, cleanState);
      } finally {
        temporaryContext?.proxy.dispose();
      }
      const relativeSpecPath = path.relative(repoRoot, specPath);
      if (errors.length > 0) {
        failedSpecs.push({ specPath: relativeSpecPath, errors });
        for (const error of errors) logError(`[parity] ${error}`);
      } else {
        passedSpecs.push(relativeSpecPath);
        log(`[parity] ${relativeSpecPath} passed`);
      }
    }
  } finally {
    for (const { proxy } of proxyContexts.values()) proxy.dispose();
    await cassette.close();
  }
  if (args.outputJsonPath) {
    const outputPath = path.resolve(repoRoot, args.outputJsonPath);
    await mkdir(path.dirname(outputPath), { recursive: true });
    await writeFile(
      outputPath,
      `${JSON.stringify(
        {
          schemaVersion: 1,
          selectedSpecs: specPaths.map((specPath) => path.relative(repoRoot, specPath)),
          passedSpecs,
          failedSpecs,
        },
        null,
        2,
      )}\n`,
      'utf8',
    );
  }
  if (failedSpecs.length > 0) {
    logError(`[parity] ${failedSpecs.length}/${specPaths.length} spec(s) failed`);
  }
  if (failedSpecs.length > 0) {
    process.exit(1);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
