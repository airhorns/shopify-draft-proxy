import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { readFile, readdir } from 'node:fs/promises';
import { existsSync, appendFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  createDraftProxy,
  type DraftProxy,
  type DraftProxyRequest,
  type DraftProxyStateDump,
} from '../js/src/index.js';
import {
  type RecordedUpstreamCall,
  recordedCallMatchesBody,
  formatRecordedCallMismatch,
  stableJson,
} from './parity-cassette.js';

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
  apiVersion?: string;
  headers?: Record<string, string>;
};

type ProxyUploadSpec = {
  method?: string;
  path: unknown;
  body?: unknown;
  headers?: Record<string, string>;
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
  selectedPaths?: string[];
  excludedPaths?: string[];
  expectedDifferences?: ExpectedDifference[];
  preserveProxyState?: boolean;
};

type ExpectedDifference = {
  path: string;
  matcher?: string;
  ignore?: true;
  reason: string;
};

type ParitySpec = {
  scenarioId: string;
  operationNames?: string[];
  liveCaptureFiles?: string[];
  proxyRequest?: ProxyRequestSpec;
  comparison?: {
    expectedDifferences?: ExpectedDifference[];
    targets?: ComparisonTarget[];
  };
};

type ProxyResponse = { status: number; body: unknown };

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const paritySpecRoot = path.join(repoRoot, 'config', 'parity-specs');
const defaultAdminApiVersion = '2026-04';

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
  for (const specPath of args.specPaths)
    specPaths.push(path.isAbsolute(specPath) ? specPath : path.resolve(repoRoot, specPath));
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

function tokenizeJsonPathWithWildcards(jsonPath: string): string[] {
  if (!jsonPath.startsWith('$')) throw new Error(`Unsupported JSONPath (must start with $): ${jsonPath}`);
  const parts: string[] = [];
  const pattern = /\.([^.[\]]+)|\[(\d+)\]|\[\*\]/gu;
  for (const match of jsonPath.matchAll(pattern)) {
    if (match[1] !== undefined) parts.push(match[1]);
    else if (match[2] !== undefined) parts.push(match[2]);
    else parts.push('*');
  }
  return parts;
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
      return getPath(primaryResponse.body, object['fromPrimaryProxyPath']);
    }
    if (typeof object['fromPreviousProxyPath'] === 'string') {
      if (previousResponse === null)
        throw new Error('fromPreviousProxyPath used before a previous proxy response exists');
      return getPath(previousResponse.body, object['fromPreviousProxyPath']);
    }
    if (typeof object['fromCapturePath'] === 'string') return getPath(capture, object['fromCapturePath']);
    if (typeof object['fromProxyResponse'] === 'string' && typeof object['path'] === 'string') {
      const response = namedResponses.get(object['fromProxyResponse']);
      if (!response) throw new Error(`fromProxyResponse references unknown target: ${object['fromProxyResponse']}`);
      return getPath(response.body, object['path']);
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

function collectHydratableInventoryIds(value: unknown, ids = new Set<string>()): Set<string> {
  if (Array.isArray(value)) {
    for (const entry of value) collectHydratableInventoryIds(entry, ids);
    return ids;
  }
  if (typeof value !== 'object' || value === null) return ids;
  for (const [key, entry] of Object.entries(value)) {
    if (
      typeof entry === 'string' &&
      (key === 'inventoryItemId' || key === 'id' || key === 'inventoryLevelId') &&
      (entry.startsWith('gid://shopify/InventoryItem/') || entry.startsWith('gid://shopify/InventoryLevel/'))
    ) {
      ids.add(entry);
    }
    collectHydratableInventoryIds(entry, ids);
  }
  return ids;
}

async function hydrateInventoryNodes(
  proxy: DraftProxy,
  request: {
    variables: Record<string, unknown>;
    headers: Record<string, string>;
    path: string;
  },
): Promise<void> {
  const ids = [...collectHydratableInventoryIds(request.variables)].sort();
  if (ids.length === 0) return;
  await sendProxyRequest(proxy, {
    path: request.path,
    headers: request.headers,
    query:
      'query ProductsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { ... on InventoryItem { id tracked requiresShipping countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode measurement { weight { value unit } } variant { id title inventoryQuantity selectedOptions { name value } product { id title handle status totalInventory tracksInventory } } inventoryLevels(first: 10, includeInactive: true) { nodes { id isActive location { id name } quantities(names: ["available", "on_hand", "committed", "incoming", "reserved"]) { name quantity updatedAt } } } } ... on InventoryLevel { id isActive location { id name } quantities(names: ["available", "on_hand", "committed", "incoming", "reserved"]) { name quantity updatedAt } item { id tracked requiresShipping variant { id title inventoryQuantity selectedOptions { name value } product { id title handle status totalInventory tracksInventory } } inventoryLevels(first: 10, includeInactive: true) { nodes { id isActive location { id name } quantities(names: ["available", "on_hand", "committed", "incoming", "reserved"]) { name quantity updatedAt } } } } } } }',
    variables: { ids },
  });
}

async function loadRequest(
  request: ProxyRequestSpec | undefined,
  capture: unknown,
  primaryResponse: ProxyResponse | null,
  previousResponse: ProxyResponse | null,
  namedResponses: Map<string, ProxyResponse>,
): Promise<{
  query: string;
  variables: Record<string, unknown>;
  headers: Record<string, string>;
  path: string;
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

  variables = resolveSpecialVariables(variables, capture, primaryResponse, previousResponse, namedResponses) as Record<
    string,
    unknown
  >;
  return {
    query,
    variables,
    headers: request.headers ?? {},
    path: `/admin/api/${request.apiVersion ?? defaultAdminApiVersion}/graphql.json`,
  };
}

type LoadedProxyRequest = {
  query: string;
  variables: Record<string, unknown>;
  headers: Record<string, string>;
  path: string;
};

type CassetteServer = {
  origin: string;
  setCalls: (calls: RecordedUpstreamCall[]) => void;
  setFallbackResponse: (response: ProxyResponse | null, request?: LoadedProxyRequest | null) => void;
  consumed: () => number;
  expected: () => number;
  close: () => Promise<void>;
};

async function startCassetteServer(): Promise<CassetteServer> {
  let calls: RecordedUpstreamCall[] = [];
  let fallbackResponse: { response: ProxyResponse; call: RecordedUpstreamCall } | null = null;
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
      const matchedIndex = calls.findIndex(
        (call, callIndex) => !consumedCalls.has(callIndex) && recordedCallMatchesBody(call, body),
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
      if (fallbackResponse !== null && recordedCallMatchesBody(fallbackResponse.call, body)) {
        fallbackCount += 1;
        response.statusCode = fallbackResponse.response.status;
        response.setHeader('content-type', 'application/json');
        response.end(JSON.stringify(fallbackResponse.response.body));
        return;
      }
      response.statusCode = 500;
      response.setHeader('content-type', 'application/json');
      response.end(JSON.stringify({ errors: [{ message: formatRecordedCallMismatch(body, calls, consumedCalls) }] }));
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
      fallbackCount = 0;
      consumedCalls.clear();
    },
    setFallbackResponse: (response: ProxyResponse | null, request?: LoadedProxyRequest | null) => {
      fallbackResponse =
        response && request ? { response, call: { query: request.query, variables: request.variables } } : null;
    },
    consumed: () => consumedCalls.size,
    expected: () => calls.length + fallbackCount,
    close: async () =>
      await new Promise<void>((resolveClose, reject) =>
        server.close((error) => (error ? reject(error) : resolveClose())),
      ),
  };
}

async function sendProxyRequest(
  proxy: DraftProxy,
  request: { query: string; variables: Record<string, unknown>; headers: Record<string, string>; path: string },
): Promise<ProxyResponse> {
  return await proxy.processRequest({
    method: 'POST',
    path: request.path,
    headers: { 'content-type': 'application/json', ...request.headers },
    body: { query: request.query, variables: request.variables },
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

function normalizeForTarget(value: unknown, target: ComparisonTarget): unknown {
  return applyExcludedPaths(selectPaths(value, target.selectedPaths), target.excludedPaths);
}

function captureResponseForTarget(capture: unknown, target: ComparisonTarget): ProxyResponse | null {
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
  return null;
}

function normalizedCapturePayload(value: unknown): unknown {
  if (typeof value !== 'object' || value === null) return null;
  const object = value as Record<string, unknown>;
  if (typeof object['payload'] === 'object' && object['payload'] !== null) return object['payload'];
  if (typeof object['body'] === 'object' && object['body'] !== null) return object['body'];
  return object;
}

function captureResponseForRequest(capture: unknown, request: LoadedProxyRequest): ProxyResponse | null {
  const pending: unknown[] = [capture];
  while (pending.length > 0) {
    const candidate = pending.pop();
    if (Array.isArray(candidate)) {
      pending.push(...candidate);
      continue;
    }
    if (typeof candidate !== 'object' || candidate === null) continue;
    const entry = candidate as Record<string, unknown>;
    if (
      typeof entry['query'] === 'string' &&
      (entry['query'] as string).trimEnd() === request.query.trimEnd() &&
      stableJson(entry['variables'] ?? {}) === stableJson(request.variables ?? {})
    ) {
      const response = normalizedCapturePayload(entry['response'] ?? entry['result']);
      if (response !== null) return { status: 200, body: response };
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

function matchesRule(value: unknown, rule: ExpectedDifference): boolean {
  if (rule.ignore) return true;
  const matcher = rule.matcher ?? '';
  if (matcher === 'any-string') return typeof value === 'string';
  if (matcher === 'non-empty-string') return typeof value === 'string' && value.length > 0;
  if (matcher === 'any-number') return typeof value === 'number';
  if (matcher === 'iso-timestamp') return isIsoTimestamp(value);
  if (matcher === 'jsonl-string') return isJsonlString(value);
  if (matcher === 'storefront-access-token') return typeof value === 'string' && value.length > 0;
  const gidMatch = /^shopify-gid:([A-Za-z][A-Za-z0-9]*)$/u.exec(matcher);
  if (gidMatch) return typeof value === 'string' && value.startsWith(`gid://shopify/${gidMatch[1]}/`);
  if (matcher.startsWith('exact-string:')) return value === matcher.slice('exact-string:'.length);
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

function diffValues(capture: unknown, proxy: unknown, rules: ExpectedDifference[], basePath = '$'): string[] {
  const rule = rules.find((candidate) => ruleMatchesPath(candidate.path, basePath));
  if (rule && matchesRule(proxy, rule)) return [];
  if (Object.is(capture, proxy)) return [];
  if (Array.isArray(capture) && Array.isArray(proxy)) {
    const errors: string[] = [];
    const max = Math.max(capture.length, proxy.length);
    for (let index = 0; index < max; index += 1)
      errors.push(...diffValues(capture[index], proxy[index], rules, `${basePath}[${index}]`));
    return errors;
  }
  if (isPlainObject(capture) && isPlainObject(proxy)) {
    const errors: string[] = [];
    const keys = new Set([...Object.keys(capture), ...Object.keys(proxy)]);
    for (const key of [...keys].sort())
      errors.push(...diffValues(capture[key], proxy[key], rules, `${basePath}.${key}`));
    return errors;
  }
  return [`${basePath}: expected ${JSON.stringify(capture)} got ${JSON.stringify(proxy)}`];
}

// Replay a capture's pre-existing entity declarations into the proxy store so
// local replay can resolve references the scenario's setup created before the
// requests under test (e.g. buyer-context customer display names / segment
// names). `seedCustomers` / `seedSegments` mirror the captured setup responses;
// the proxy upserts them by their captured ids. No-op when the capture declares
// no seeds, so this is inert for every spec that does not need it.
// Collect the richest projection of every draft order / order referenced by a
// capture's recorded precondition steps, keyed by gid. Precondition steps live
// under `setup` blocks — most scenarios carry a single top-level `setup`, but
// some nest several disposable ones (e.g. one `setup` per invoice-send branch
// under `recipient`/`lifecycle`). We gather every `setup` block anywhere in the
// capture and seed only from those, which keeps assertion payloads (the
// top-level mutation/downstreamRead the scenario compares against) out of the
// seed set — seeding only re-establishes pre-existing entities, never the
// behaviour under test. A draft created and then deleted within its setup
// (deletedId on a draftOrderDelete step) is dropped from the seed set, so
// not-found assertions stay faithful.
function collectSetupEntitySeeds(capture: unknown): {
  draftOrders: Record<string, unknown>[];
  orders: Record<string, unknown>[];
} {
  const draftOrders = new Map<string, Record<string, unknown>>();
  const orders = new Map<string, Record<string, unknown>>();
  const fulfillmentOrders = new Map<string, Record<string, unknown>>();
  const deletedIds = new Set<string>();
  // Setup blocks describe an entity across several steps, in chronological
  // (document) order — e.g. an order is created (`displayFinancialStatus: PAID`,
  // `totalRefundedSet: 0`), partially refunded, then re-read (`PARTIALLY_REFUNDED`,
  // `totalRefundedSet: 10`). Picking the single projection with the most keys
  // would seed the stale create-time snapshot; instead merge every projection of
  // a given id field-by-field in visit order, so a later step's value wins per
  // field and the seed reflects the entity's final pre-test state. Each projection
  // is append-only in practice, so this only ever adds keys or overrides a value a
  // later setup step deliberately changed.
  const mergeProjection = (
    map: Map<string, Record<string, unknown>>,
    id: string,
    next: Record<string, unknown>,
  ): void => {
    const prev = map.get(id);
    map.set(id, prev ? { ...prev, ...next } : next);
  };
  const visit = (node: unknown): void => {
    if (Array.isArray(node)) {
      for (const entry of node) visit(entry);
      return;
    }
    if (node === null || typeof node !== 'object') return;
    const obj = node as Record<string, unknown>;
    const id = obj['id'];
    if (typeof id === 'string') {
      if (id.startsWith('gid://shopify/DraftOrder/')) {
        mergeProjection(draftOrders, id, obj);
      } else if (id.startsWith('gid://shopify/Order/')) {
        mergeProjection(orders, id, obj);
      } else if (id.startsWith('gid://shopify/FulfillmentOrder/')) {
        mergeProjection(fulfillmentOrders, id, obj);
      }
    }
    const deletedId = obj['deletedId'];
    if (typeof deletedId === 'string') deletedIds.add(deletedId);
    for (const value of Object.values(obj)) visit(value);
  };
  // A setup step's recorded *response* often omits fields the create/update
  // *input* set (e.g. a draftOrderCreate that never re-queries `note`). Those
  // fields are genuinely part of the entity's state, so overlay the step's
  // input back onto the matching seed. Only a curated set of fields whose input
  // and entity representations coincide are overlaid, and only where the
  // response left the field absent or null — so an asserted response value is
  // never overridden. This keeps seeds faithful to the entity the setup created.
  const OVERLAY_FIELDS = ['note', 'tags', 'email', 'taxExempt', 'phone', 'customAttributes', 'poNumber'];
  const overlayInputs = (node: unknown): void => {
    if (Array.isArray(node)) {
      for (const entry of node) overlayInputs(entry);
      return;
    }
    if (node === null || typeof node !== 'object') return;
    const obj = node as Record<string, unknown>;
    const input = (obj['variables'] as Record<string, unknown> | undefined)?.['input'] as
      | Record<string, unknown>
      | undefined;
    const data = (
      (obj['mutation'] as Record<string, unknown> | undefined)?.['response'] as Record<string, unknown> | undefined
    )?.['data'] as Record<string, unknown> | undefined;
    if (input && typeof input === 'object' && data && typeof data === 'object') {
      for (const op of Object.values(data)) {
        if (op === null || typeof op !== 'object') continue;
        const entity = (op as Record<string, unknown>)['draftOrder'] ?? (op as Record<string, unknown>)['order'];
        const id = (entity as Record<string, unknown> | undefined)?.['id'];
        if (typeof id !== 'string') continue;
        const seed = draftOrders.get(id) ?? orders.get(id);
        if (!seed) continue;
        for (const fieldName of OVERLAY_FIELDS) {
          if (!(fieldName in input)) continue;
          if (seed[fieldName] === undefined || seed[fieldName] === null) {
            seed[fieldName] = input[fieldName];
          }
        }
      }
    }
    for (const value of Object.values(obj)) overlayInputs(value);
  };
  // Gather every `setup` block in the capture (top-level and nested).
  const setupBlocks: unknown[] = [];
  const findSetups = (node: unknown): void => {
    if (Array.isArray(node)) {
      for (const entry of node) findSetups(entry);
      return;
    }
    if (node === null || typeof node !== 'object') return;
    const obj = node as Record<string, unknown>;
    if ('setup' in obj) setupBlocks.push(obj['setup']);
    for (const value of Object.values(obj)) findSetups(value);
  };
  findSetups(capture);
  for (const block of setupBlocks) visit(block);
  for (const block of setupBlocks) overlayInputs(block);
  // A fulfillment order can be relocated, held, or otherwise mutated across
  // setup steps — e.g. orderCreate places it at location A, then a later
  // fulfillmentOrderMove relocates it to location B (the move's response nests
  // the moved FulfillmentOrder *outside* the order's `fulfillmentOrders`
  // connection, so the order projection alone keeps the stale create-time
  // location). Overlay the merged FulfillmentOrder projection (final pre-test
  // state — move/hold applied, since later steps win per field) back onto each
  // matching node the seeded order already declares. Only nodes the order
  // already carries are enriched, never added, so this strictly reflects the
  // entity's latest pre-test state without inventing connection members.
  const overlayFulfillmentOrders = (order: Record<string, unknown>): void => {
    const connection = order['fulfillmentOrders'] as Record<string, unknown> | undefined;
    const nodes = connection?.['nodes'];
    if (!Array.isArray(nodes)) return;
    for (let i = 0; i < nodes.length; i++) {
      const node = nodes[i] as Record<string, unknown> | null;
      const nodeId = node?.['id'];
      if (typeof nodeId !== 'string') continue;
      const merged = fulfillmentOrders.get(nodeId);
      if (!merged) continue;
      // The merged FO projection carries an `order` back-reference (the
      // hydrate selects `order { ... }`). Overlaying it onto a node that
      // lives *inside* that same order's `fulfillmentOrders.nodes` would
      // close a cycle (order -> nodes[i] -> order -> ...), which breaks
      // JSON serialization of the seed body. Keep the node's own `order`
      // (from `...node`) and overlay every other FO field.
      const { order: _omitOrderBackref, ...mergedFields } = merged;
      nodes[i] = { ...node, ...mergedFields };
    }
  };
  for (const order of orders.values()) overlayFulfillmentOrders(order);
  for (const id of deletedIds) {
    draftOrders.delete(id);
    orders.delete(id);
  }
  return { draftOrders: [...draftOrders.values()], orders: [...orders.values()] };
}

// `seedOrderCatalogFromCapture: true` declares that the scenario's local order
// catalog should be re-established from the capture's own recorded order nodes
// (the `orders` connection reads under `response` / `nextPage`). Collect every
// `gid://shopify/Order/...` projection appearing in those assertion payloads and
// merge them field-by-field by id, so the local orders/ordersCount engine has the
// full catalog to filter, sort, paginate, and count against. A status-only node
// (id/name/status) contributes those fields; a richer node (the seed/recent read)
// contributes createdAt/tags/email. Only the recorded assertion payloads are
// walked — never `upstreamCalls` or `setup` — so the seeded catalog stays
// faithful to exactly the orders the scenario observes.
function collectOrderCatalogSeeds(capture: unknown): Record<string, unknown>[] {
  const record = capture as Record<string, unknown>;
  if (record['seedOrderCatalogFromCapture'] !== true) return [];
  const orders = new Map<string, Record<string, unknown>>();
  const visit = (node: unknown): void => {
    if (Array.isArray(node)) {
      for (const entry of node) visit(entry);
      return;
    }
    if (node === null || typeof node !== 'object') return;
    const obj = node as Record<string, unknown>;
    const id = obj['id'];
    if (typeof id === 'string' && id.startsWith('gid://shopify/Order/')) {
      const seed = orders.get(id) ?? {};
      for (const [key, value] of Object.entries(obj)) {
        // First non-null projection of a field wins; overlapping projections of
        // the same order carry identical values, so this only ever fills gaps.
        if (seed[key] === undefined || seed[key] === null) seed[key] = value;
      }
      orders.set(id, seed);
    }
    for (const value of Object.values(obj)) visit(value);
  };
  visit(record['response']);
  visit(record['nextPage']);
  return [...orders.values()];
}

function collectSetupGiftCardSeeds(record: Record<string, unknown>): Record<string, unknown>[] {
  const cards = new Map<string, Record<string, unknown>>();
  const setup = Array.isArray(record['setup']) ? record['setup'] : [];
  for (const rawStep of setup) {
    if (!rawStep || typeof rawStep !== 'object') continue;
    const step = rawStep as Record<string, unknown>;
    const response = step['response'] as Record<string, unknown> | undefined;
    const payload = response?.['payload'] as Record<string, unknown> | undefined;
    const data = payload?.['data'] as Record<string, unknown> | undefined;
    if (!data) continue;
    for (const rawPayload of Object.values(data)) {
      if (!rawPayload || typeof rawPayload !== 'object') continue;
      const mutationPayload = rawPayload as Record<string, unknown>;
      const rawGiftCard = mutationPayload['giftCard'];
      if (!rawGiftCard || typeof rawGiftCard !== 'object') continue;
      const giftCard = rawGiftCard as Record<string, unknown>;
      const id = giftCard['id'];
      if (typeof id !== 'string') continue;
      const merged = { ...cards.get(id), ...giftCard };
      if (typeof mutationPayload['giftCardCode'] === 'string') {
        merged['giftCardCode'] = mutationPayload['giftCardCode'];
      }
      cards.set(id, merged);
    }
  }
  return [...cards.values()];
}

// Recursively collect opaque InventoryLevel connection cursors (level gid -> cursor)
// from any recorded `inventoryLevels { edges { cursor node { id } } }` shape in a
// capture (top-level data or upstream-call response bodies). These Relay pagination
// tokens encode Shopify's internal row ids and cannot be reconstructed from store
// state, so re-seeding them lets the local inventory-level connection renderer replay
// the live cursors verbatim when a read projects edges/pageInfo.
function collectInventoryLevelCursors(node: unknown, out: Record<string, string>): void {
  if (Array.isArray(node)) {
    for (const item of node) collectInventoryLevelCursors(item, out);
    return;
  }
  if (!node || typeof node !== 'object') return;
  const record = node as Record<string, unknown>;
  const cursor = record['cursor'];
  const inner = record['node'] as Record<string, unknown> | undefined;
  const id = inner?.['id'];
  if (typeof cursor === 'string' && typeof id === 'string' && id.includes('/InventoryLevel/')) {
    out[id] = cursor;
  }
  for (const value of Object.values(record)) collectInventoryLevelCursors(value, out);
}

function shouldDeriveGenericResourceSeeds(record: Record<string, unknown>): boolean {
  return Array.isArray(record['metafieldBatches']);
}

function shouldSeedSetupGiftCards(spec: ParitySpec): boolean {
  return !(spec.operationNames ?? []).includes('giftCardCreate');
}

async function seedPreconditionsFromCapture(proxy: DraftProxy, capture: unknown, spec: ParitySpec): Promise<void> {
  const record = capture as Record<string, unknown>;
  const customers = Array.isArray(record['seedCustomers']) ? record['seedCustomers'] : [];
  const segments = Array.isArray(record['seedSegments']) ? record['seedSegments'] : [];
  const products = Array.isArray(record['seedProducts']) ? record['seedProducts'] : [];
  const productVariants = Array.isArray(record['seedProductVariants']) ? record['seedProductVariants'] : [];
  const collections = Array.isArray(record['seedCollections']) ? record['seedCollections'] : [];
  const genericSeed =
    record['seed'] && typeof record['seed'] === 'object' && !Array.isArray(record['seed'])
      ? (record['seed'] as Record<string, unknown>)
      : null;
  const includesSeedId = (entries: unknown[], id: string): boolean =>
    entries.some((entry) => entry && typeof entry === 'object' && (entry as Record<string, unknown>)['id'] === id);
  if (genericSeed && shouldDeriveGenericResourceSeeds(record)) {
    const productId = typeof genericSeed['productId'] === 'string' ? genericSeed['productId'] : null;
    const variantId = typeof genericSeed['variantId'] === 'string' ? genericSeed['variantId'] : null;
    const collectionId = typeof genericSeed['collectionId'] === 'string' ? genericSeed['collectionId'] : null;
    if (productId && !includesSeedId(products, productId)) {
      products.push({ id: productId });
    }
    if (variantId && productId && !includesSeedId(productVariants, variantId)) {
      const variantTail = variantId.split('/').pop() ?? 'seed';
      productVariants.push({
        id: variantId,
        productId,
        inventoryItem: { id: `gid://shopify/InventoryItem/${variantTail}` },
      });
    }
    if (collectionId && !includesSeedId(collections, collectionId)) {
      collections.push({ id: collectionId });
    }
  }
  // `seedPublications` declares the store's base/default publications (id + name);
  // the proxy derives each backing channel and drives the local publication/channel
  // roots from them. Per-resource membership rides on the `publicationIds` of the
  // seeded products/collections.
  const publications = Array.isArray(record['seedPublications']) ? record['seedPublications'] : [];
  const discounts = Array.isArray(record['seedDiscounts']) ? record['seedDiscounts'] : [];
  const giftCards = [
    ...(Array.isArray(record['seedGiftCards']) ? record['seedGiftCards'] : []),
    ...(shouldSeedSetupGiftCards(spec) ? collectSetupGiftCardSeeds(record) : []),
  ];
  // Draft orders / orders are derived from the recorded `setup` precondition steps
  // (and may be augmented by explicit `seedDraftOrders` / `seedOrders` arrays).
  const setupSeeds = collectSetupEntitySeeds(record);
  // Some captures carry a single pre-existing entity as `seedOrder` /
  // `seedDraftOrder` (the realistic order an order-edit / return workflow runs
  // against) rather than an array. Treat the singular form as a one-element seed
  // alongside the array form and the setup-derived projections.
  const singletonSeed = (key: string): Record<string, unknown>[] =>
    record[key] && typeof record[key] === 'object' && !Array.isArray(record[key])
      ? [record[key] as Record<string, unknown>]
      : [];
  const draftOrders = [
    ...(Array.isArray(record['seedDraftOrders']) ? record['seedDraftOrders'] : []),
    ...singletonSeed('seedDraftOrder'),
    ...setupSeeds.draftOrders,
  ];
  const orders = [
    ...(Array.isArray(record['seedOrders']) ? record['seedOrders'] : []),
    ...singletonSeed('seedOrder'),
    ...setupSeeds.orders,
    ...collectOrderCatalogSeeds(record),
  ];
  // `seedOrderEditVariants` mirrors the store catalog entries an order-edit
  // `orderEditAddVariant` resolves against (variant id → product title / sku /
  // unit price). Re-establishing the catalog lets the local edit engine build
  // the added calculated line item from store state instead of echoing the
  // recorded response.
  let orderEditVariants = Array.isArray(record['seedOrderEditVariants']) ? record['seedOrderEditVariants'] : [];
  // When a capture does not carry an explicit `seedOrderEditVariants` array, derive
  // the order-edit variant catalog from the recorded `productVariant` hydration
  // calls. Those hydrate responses are real store state (variant id, product title,
  // sku, unit price) that `orderEditAddVariant` resolves the added calculated line
  // item against — re-establishing the catalog lets the local edit engine compute
  // the added line from store state instead of relying on a passthrough echo of the
  // recorded mutation response.
  if (orderEditVariants.length === 0 && Array.isArray(record['upstreamCalls'])) {
    const derived: Record<string, unknown>[] = [];
    for (const rawCall of record['upstreamCalls'] as unknown[]) {
      const call = rawCall as Record<string, unknown> | null;
      const response = call?.['response'] as Record<string, unknown> | undefined;
      const body = (response?.['body'] ?? response) as Record<string, unknown> | undefined;
      const data = body?.['data'] as Record<string, unknown> | undefined;
      const variant = data?.['productVariant'] as Record<string, unknown> | undefined;
      const id = variant?.['id'];
      if (variant && typeof id === 'string' && !derived.some((entry) => entry['id'] === id)) {
        const product = variant['product'] as Record<string, unknown> | undefined;
        derived.push({
          id,
          // The calculated line-item title mirrors the product title (the variant
          // title is carried separately), so prefer it when the hydrate nested it.
          title: (product?.['title'] as string | undefined) ?? variant['title'] ?? null,
          sku: variant['sku'] ?? null,
          price: variant['price'] ?? null,
        });
      }
    }
    orderEditVariants = derived;
  }
  // `seedOrderEditAuthor` is the identity of the actor (app / staff) that performs
  // the order edit. Shopify records the committed "<author> edited this order."
  // history event against whoever held the editing session; that identity is
  // session/store state, not anything derivable from the rest of the capture.
  // Re-establishing it lets the local commit engine compute the edited-order event
  // message generically from the seeded author instead of echoing the recorded text.
  const orderEditAuthor =
    typeof record['seedOrderEditAuthor'] === 'string' ? (record['seedOrderEditAuthor'] as string) : null;
  // `seedSegmentCatalog` mirrors the captured segment-catalog read roots (filters,
  // filter/value suggestions, migrations, the segments connection, and the live
  // total count) so local replay can serve their recorded cursors/pageInfo that
  // cannot be reconstructed from arbitrary store state.
  const segmentCatalog =
    record['seedSegmentCatalog'] && typeof record['seedSegmentCatalog'] === 'object'
      ? (record['seedSegmentCatalog'] as Record<string, unknown>)
      : null;
  // `seedCustomersCount` mirrors the live shop's total customer count so the
  // `customersCount` read root can report the store-specific baseline (which is
  // not reconstructable from the staged customers) and track deletions/merges as
  // `base - deletions`.
  const customersCount =
    typeof record['seedCustomersCount'] === 'number' ? (record['seedCustomersCount'] as number) : null;
  // `seedCustomerOrders` maps a customer id to the recorded order nodes attached to
  // it (each optionally carrying an opaque `__cursor`). Customer reads and merges
  // that reparent orders resolve these from store state; the live connection cursors
  // can't be reconstructed locally, so re-seeding preserves them for downstream reads.
  const customerOrders =
    record['seedCustomerOrders'] && typeof record['seedCustomerOrders'] === 'object'
      ? (record['seedCustomerOrders'] as Record<string, unknown>)
      : null;
  // `seedCollectionCatalog` mirrors `seedSegmentCatalog`: recorded top-level
  // `collections(query:, sortKey:)` connection snapshots keyed by GraphQL alias.
  // Their opaque search-index cursors (title case folding, SQL-datetime sort keys)
  // cannot be reconstructed from store state, so the catalog read projects the
  // requested selection over these recorded connections for local replay.
  const collectionCatalog =
    record['seedCollectionCatalog'] && typeof record['seedCollectionCatalog'] === 'object'
      ? (record['seedCollectionCatalog'] as Record<string, unknown>)
      : null;
  // Recover opaque InventoryLevel connection cursors recorded anywhere in the capture
  // (the captured response under test plus every upstream-call body) so variant /
  // inventory-item reads can replay them on their `inventoryLevels` edges/pageInfo.
  const inventoryLevelCursors: Record<string, string> = {};
  collectInventoryLevelCursors(record['data'], inventoryLevelCursors);
  if (Array.isArray(record['upstreamCalls'])) {
    for (const rawCall of record['upstreamCalls'] as unknown[]) {
      const call = rawCall as Record<string, unknown> | null;
      const response = call?.['response'] as Record<string, unknown> | undefined;
      const body = (response?.['body'] ?? response) as Record<string, unknown> | undefined;
      collectInventoryLevelCursors(body?.['data'], inventoryLevelCursors);
    }
  }
  const hasInventoryLevelCursors = Object.keys(inventoryLevelCursors).length > 0;
  if (
    customers.length === 0 &&
    segments.length === 0 &&
    products.length === 0 &&
    productVariants.length === 0 &&
    collections.length === 0 &&
    publications.length === 0 &&
    discounts.length === 0 &&
    giftCards.length === 0 &&
    draftOrders.length === 0 &&
    orders.length === 0 &&
    orderEditVariants.length === 0 &&
    orderEditAuthor === null &&
    segmentCatalog === null &&
    customersCount === null &&
    customerOrders === null &&
    collectionCatalog === null &&
    !hasInventoryLevelCursors
  )
    return;
  await proxy.processRequest({
    method: 'POST',
    path: '/__meta/seed',
    body: {
      customers,
      segments,
      products,
      productVariants,
      collections,
      publications,
      discounts,
      giftCards,
      draftOrders,
      orders,
      orderEditVariants,
      ...(orderEditAuthor ? { orderEditAuthor } : {}),
      ...(segmentCatalog ? { segmentCatalog } : {}),
      ...(customersCount !== null ? { customersCount } : {}),
      ...(customerOrders !== null ? { customerOrders } : {}),
      ...(collectionCatalog ? { collectionCatalog } : {}),
      ...(hasInventoryLevelCursors ? { inventoryLevelCursors } : {}),
    },
  });
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
  const upstreamCalls = (capture['upstreamCalls'] ?? []) as RecordedUpstreamCall[];
  cassette.setCalls(upstreamCalls);
  proxy.restoreState(cleanState);
  await proxy.processRequest({ method: 'POST', path: '/__meta/reset' });
  await seedPreconditionsFromCapture(proxy, capture, spec);
  const failures: string[] = [];
  let primaryResponse: ProxyResponse | null = null;
  let previousResponse: ProxyResponse | null = null;
  const namedResponses = new Map<string, ProxyResponse>();
  try {
    const primaryRequest = await loadRequest(spec.proxyRequest, capture, null, null, namedResponses);
    if (primaryRequest !== null) {
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
      await hydrateInventoryNodes(proxy, primaryRequest);
      primaryResponse = await sendProxyRequest(proxy, primaryRequest);
      previousResponse = primaryResponse;
    }
    let mainState = proxy.dumpState('1970-01-01T00:00:00.000Z');

    for (const target of spec.comparison?.targets ?? []) {
      let proxySource: unknown;
      if (target.isolatedProxy) {
        cassette.setCalls(upstreamCalls);
        await proxy.processRequest({ method: 'POST', path: '/__meta/reset' });
        await seedPreconditionsFromCapture(proxy, capture, spec);
        primaryResponse = null;
        previousResponse = null;
        namedResponses.clear();
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
        );
        if (request === null) throw new Error(`${target.name}: target proxyRequest did not resolve to a request`);
        cassette.setFallbackResponse(captureResponseForTarget(capture, target), request);
        await hydrateInventoryNodes(proxy, request);
        const targetResponse = await sendProxyRequest(proxy, request);
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
      const captureValue = normalizeForTarget(getPath(capture, target.capturePath), target);
      const proxyPath = target.proxyPath ?? target.proxyStatePath ?? target.proxyLogPath ?? '$';
      const proxyValue = normalizeForTarget(getPath(proxySource, proxyPath), target);
      const rules = [...(spec.comparison?.expectedDifferences ?? []), ...(target.expectedDifferences ?? [])];
      const diffs = diffValues(captureValue, proxyValue, rules);
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
  const cleanState = proxy.dumpState('1970-01-01T00:00:00.000Z');

  let failures = 0;
  try {
    for (const specPath of specPaths) {
      const errors = await runSpec(specPath, args.debug, proxy, cassette, cleanState);
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
