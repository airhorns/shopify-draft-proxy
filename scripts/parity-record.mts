// Parity cassette recorder.
//
// Boots the Rust DraftProxy in LiveHybrid mode pointed at a local recording
// upstream server, plays the parity spec's primary + target requests through
// it, and rewrites the capture file with an `upstreamCalls` cassette: every
// upstream GraphQL call the proxy makes while serving the spec, in order, with
// its (operationName, variables, response) tuple.
//
// The recording transport forwards each intercepted upstream call to the real
// Shopify Admin GraphQL endpoint using the existing OAuth flow
// (scripts/shopify-conformance-auth.mts) and stores the response so it can
// be replayed later by the parity runner.
//
// Usage:
//   pnpm parity:record <scenario-id>
//   pnpm parity:record --all
//   pnpm parity:record --spec config/parity-specs/customers/customer-detail-parity-plan.json

// @ts-nocheck

import 'dotenv/config';

import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { readdir } from 'node:fs/promises';
import { createServer } from 'node:http';
import { fileURLToPath } from 'node:url';
import { dirname, isAbsolute, relative, resolve } from 'node:path';

import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..');
const shimEntrypoint = resolve(repoRoot, 'js/src/index.ts');

type RecordedCall = {
  operationName: string;
  variables: unknown;
  query: string;
  response: { status: number; body: unknown };
};

type SpecTargetRequest = {
  documentPath?: string;
  documentCapturePath?: string;
  variables?: Record<string, unknown>;
  variablesPath?: string;
  variablesCapturePath?: string;
  apiVersion?: string;
  headers?: Record<string, string>;
};

type SpecTarget = {
  name?: string;
  proxyRequest?: SpecTargetRequest;
};

type RecordedResponse = { status: number; body: Record<string, unknown> };

type ParitySpec = {
  scenarioId: string;
  liveCaptureFiles?: string[];
  proxyRequest?: SpecTargetRequest;
  comparison?: {
    targets?: SpecTarget[];
  };
};

type RecordOptions = {
  specPath: string;
  apiVersion: string;
  adminOrigin: string;
  accessToken: string;
};

function log(message: string): void {
  // oxlint-disable-next-line no-console -- CLI tool intentionally writes status to stdout.
  console.log(message);
}

function logError(message: string): void {
  // oxlint-disable-next-line no-console -- CLI tool intentionally writes errors to stderr.
  console.error(message);
}

function parseArgs(argv: string[]): { scenarioIds: string[]; specPaths: string[]; all: boolean } {
  const scenarioIds: string[] = [];
  const specPaths: string[] = [];
  let all = false;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--all') {
      all = true;
    } else if (arg === '--spec') {
      const next = argv[++i];
      if (!next) throw new Error('--spec requires a path argument');
      specPaths.push(next);
    } else if (!arg.startsWith('--')) {
      scenarioIds.push(arg);
    } else {
      throw new Error(`Unknown flag: ${arg}`);
    }
  }
  return { scenarioIds, specPaths, all };
}

async function findAllSpecPaths(): Promise<string[]> {
  const root = resolve(repoRoot, 'config/parity-specs');
  const out: string[] = [];
  async function walk(dir: string): Promise<void> {
    const entries = await readdir(dir, { withFileTypes: true });
    for (const entry of entries) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) {
        await walk(path);
      } else if (entry.isFile() && entry.name.endsWith('.json')) {
        out.push(path);
      }
    }
  }
  await walk(root);
  return out;
}

async function findSpecForScenario(scenarioId: string): Promise<string> {
  const all = await findAllSpecPaths();
  for (const path of all) {
    try {
      const parsed = JSON.parse(readFileSync(path, 'utf8')) as ParitySpec;
      if (parsed.scenarioId === scenarioId) return path;
    } catch {
      continue;
    }
  }
  throw new Error(`No parity spec with scenarioId "${scenarioId}" found under config/parity-specs/`);
}

function loadDocumentVariablesAndHeaders(
  request: SpecTargetRequest | undefined,
  capture: unknown,
): {
  document: string;
  variables: Record<string, unknown>;
  headers: Record<string, string>;
  apiVersion?: string;
} | null {
  if (!request || (!request.documentPath && !request.documentCapturePath)) return null;
  let document: string;
  if (request.documentCapturePath) {
    const capturedDocument = resolveJsonPath(capture, request.documentCapturePath);
    if (typeof capturedDocument !== 'string') {
      throw new Error(`Spec references missing captured document: ${request.documentCapturePath}`);
    }
    document = capturedDocument;
  } else {
    const documentPath = resolve(repoRoot, request.documentPath);
    if (!existsSync(documentPath)) {
      throw new Error(`Spec references missing document: ${request.documentPath}`);
    }
    document = readFileSync(documentPath, 'utf8');
  }

  let variables: Record<string, unknown> = {};
  if (request.variablesCapturePath) {
    variables = (resolveJsonPath(capture, request.variablesCapturePath) ?? {}) as Record<string, unknown>;
  } else if (request.variablesPath) {
    const variablesPath = resolve(repoRoot, request.variablesPath);
    if (!existsSync(variablesPath)) {
      throw new Error(`Spec references missing variables: ${request.variablesPath}`);
    }
    variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as Record<string, unknown>;
  } else if (request.variables) {
    variables = request.variables;
  }
  return { document, variables, headers: request.headers ?? {}, apiVersion: request.apiVersion };
}

function resolveJsonPath(value: unknown, path: string): unknown {
  if (!path.startsWith('$')) {
    throw new Error(`Unsupported JSONPath (must start with $): ${path}`);
  }
  let cursor: unknown = value;
  // Tokenise: $.a.b[0].c → ['a','b','0','c']
  const stripped = path.slice(1);
  const parts: string[] = [];
  let buf = '';
  for (let i = 0; i < stripped.length; i++) {
    const ch = stripped[i];
    if (ch === '.') {
      if (buf.length > 0) {
        parts.push(buf);
        buf = '';
      }
    } else if (ch === '[') {
      if (buf.length > 0) {
        parts.push(buf);
        buf = '';
      }
      const close = stripped.indexOf(']', i);
      if (close < 0) throw new Error(`Malformed JSONPath: ${path}`);
      parts.push(stripped.slice(i + 1, close));
      i = close;
    } else {
      buf += ch;
    }
  }
  if (buf.length > 0) parts.push(buf);

  for (const part of parts) {
    if (cursor === null || cursor === undefined) return undefined;
    if (Array.isArray(cursor)) {
      const idx = Number.parseInt(part, 10);
      cursor = cursor[idx];
    } else if (typeof cursor === 'object') {
      cursor = (cursor as Record<string, unknown>)[part];
    } else {
      return undefined;
    }
  }
  return cursor;
}

function requireJsonPath(value: unknown, path: string, context: string): unknown {
  const resolved = resolveJsonPath(value, path);
  if (resolved === undefined) {
    throw new Error(`${context} did not resolve JSONPath: ${path}`);
  }
  return resolved;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function substituteVariables(
  template: unknown,
  context: {
    capture: unknown;
    primaryResponse?: RecordedResponse;
    previousResponse?: RecordedResponse;
    responsesByName: Map<string, RecordedResponse>;
  },
): unknown {
  if (Array.isArray(template)) {
    return template.map((item) => substituteVariables(item, context));
  }
  if (!isPlainObject(template)) {
    return template;
  }

  const entries = Object.entries(template);
  if (entries.length === 1) {
    const [[key, value]] = entries;
    if (key === 'fromPrimaryProxyPath' && typeof value === 'string') {
      if (!context.primaryResponse) throw new Error(`fromPrimaryProxyPath used before primary response: ${value}`);
      return requireJsonPath(context.primaryResponse.body, value, 'primary response');
    }
    if (key === 'fromPreviousProxyPath' && typeof value === 'string') {
      if (!context.previousResponse) throw new Error(`fromPreviousProxyPath used before previous response: ${value}`);
      return requireJsonPath(context.previousResponse.body, value, 'previous response');
    }
    if (key === 'fromCapturePath' && typeof value === 'string') {
      return requireJsonPath(context.capture, value, 'capture');
    }
  }

  const responseName = template.fromProxyResponse;
  const responsePath = template.path;
  if (typeof responseName === 'string' && typeof responsePath === 'string') {
    const named = context.responsesByName.get(responseName);
    if (!named) throw new Error(`fromProxyResponse target not found: ${responseName}`);
    return requireJsonPath(named.body, responsePath, `proxy response '${responseName}'`);
  }

  return Object.fromEntries(entries.map(([key, value]) => [key, substituteVariables(value, context)]));
}

function setJsonPath(root: Record<string, unknown>, path: string, value: unknown): void {
  if (!path.startsWith('$')) {
    throw new Error(`Unsupported JSONPath (must start with $): ${path}`);
  }
  const stripped = path.slice(1);
  const parts: string[] = [];
  let buf = '';
  for (let i = 0; i < stripped.length; i++) {
    const ch = stripped[i];
    if (ch === '.') {
      if (buf.length > 0) {
        parts.push(buf);
        buf = '';
      }
    } else if (ch === '[') {
      if (buf.length > 0) {
        parts.push(buf);
        buf = '';
      }
      const close = stripped.indexOf(']', i);
      if (close < 0) throw new Error(`Malformed JSONPath: ${path}`);
      parts.push(stripped.slice(i + 1, close));
      i = close;
    } else {
      buf += ch;
    }
  }
  if (buf.length > 0) parts.push(buf);
  if (parts.length === 0) {
    throw new Error(`Cannot set root via JSONPath: ${path}`);
  }
  let cursor: Record<string, unknown> | unknown[] = root;
  for (let i = 0; i < parts.length - 1; i++) {
    const part = parts[i];
    if (Array.isArray(cursor)) {
      const idx = Number.parseInt(part, 10);
      const next = cursor[idx];
      if (next === undefined || next === null || (typeof next !== 'object' && !Array.isArray(next))) {
        cursor[idx] = {};
      }
      cursor = cursor[idx] as Record<string, unknown> | unknown[];
    } else if (typeof cursor === 'object' && cursor !== null) {
      const obj = cursor as Record<string, unknown>;
      const next = obj[part];
      if (next === undefined || next === null || typeof next !== 'object') {
        obj[part] = {};
      }
      cursor = obj[part] as Record<string, unknown> | unknown[];
    } else {
      throw new Error(`Cannot traverse JSONPath ${path} at ${part}: parent is not an object`);
    }
  }
  const last = parts[parts.length - 1];
  if (Array.isArray(cursor)) {
    const idx = Number.parseInt(last, 10);
    cursor[idx] = value;
  } else {
    (cursor as Record<string, unknown>)[last] = value;
  }
}

function rewriteCapture(
  captureFile: string,
  calls: RecordedCall[],
  rewrites: { capturePath: string; value: unknown }[],
): void {
  const source = readFileSync(captureFile, 'utf8');
  let parsed: Record<string, unknown>;
  try {
    parsed = JSON.parse(source) as Record<string, unknown>;
  } catch {
    throw new Error(`Capture file is not valid JSON: ${captureFile}`);
  }
  for (const { capturePath, value } of rewrites) {
    setJsonPath(parsed, capturePath, value);
  }
  parsed.upstreamCalls = calls;
  writeFileSync(captureFile, JSON.stringify(parsed, null, 2) + '\n', 'utf8');
}

async function startRecordingUpstream(opts: {
  adminOrigin: string;
  authHeaders: Record<string, string>;
  calls: RecordedCall[];
}): Promise<{ origin: string; close: () => Promise<void> }> {
  const server = createServer((request, response) => {
    let body = '';
    request.setEncoding('utf8');
    request.on('data', (chunk) => (body += chunk));
    request.on('end', async () => {
      const parsed = JSON.parse(body || '{}') as {
        operationName?: string;
        query?: string;
        variables?: Record<string, unknown>;
      };
      try {
        const upstreamUrl = new URL(request.url ?? '/', opts.adminOrigin);
        const upstreamResponse = await fetch(upstreamUrl, {
          method: request.method ?? 'POST',
          headers: {
            'content-type': String(request.headers['content-type'] ?? 'application/json'),
            ...opts.authHeaders,
          },
          body,
        });
        const responseBody = await upstreamResponse.text();
        const responsePayload = JSON.parse(responseBody || '{}') as unknown;
        opts.calls.push({
          operationName: parsed.operationName ?? extractOperationName(parsed.query ?? '') ?? '',
          variables: parsed.variables ?? {},
          query: parsed.query ?? '',
          response: { status: upstreamResponse.status, body: responsePayload },
        });
        response.statusCode = upstreamResponse.status;
        response.setHeader('content-type', 'application/json');
        response.end(responseBody);
      } catch (err) {
        response.statusCode = 502;
        response.setHeader('content-type', 'application/json');
        response.end(JSON.stringify({ errors: [{ message: (err as Error).message }] }));
      }
    });
  });
  await new Promise<void>((resolveListen) => server.listen(0, '127.0.0.1', resolveListen));
  const address = server.address();
  if (address === null || typeof address === 'string') throw new Error('Failed to start recording upstream server');
  return {
    origin: `http://127.0.0.1:${address.port}`,
    close: () =>
      new Promise<void>((resolveClose, rejectClose) =>
        server.close((err) => (err ? rejectClose(err) : resolveClose())),
      ),
  };
}

function extractOperationName(query: string): string | undefined {
  const match = query.match(/\b(?:query|mutation|subscription)\s+([A-Za-z_][A-Za-z0-9_]*)/);
  return match ? match[1] : undefined;
}

async function recordSpec(opts: RecordOptions): Promise<void> {
  const specSource = readFileSync(opts.specPath, 'utf8');
  const spec = JSON.parse(specSource) as ParitySpec;

  if (!spec.liveCaptureFiles || spec.liveCaptureFiles.length === 0) {
    throw new Error(`Spec ${spec.scenarioId} has no liveCaptureFiles; cannot determine where to write upstreamCalls.`);
  }
  const captureFile = resolve(repoRoot, spec.liveCaptureFiles[0]);
  if (!existsSync(captureFile)) {
    throw new Error(`Capture file does not exist: ${captureFile}. Run the corresponding capture script first.`);
  }
  const capture = JSON.parse(readFileSync(captureFile, 'utf8'));

  const calls: RecordedCall[] = [];
  let rewriteCaptureNow: (() => void) | null = null;
  let proxy: { processGraphQLRequest: (...args: unknown[]) => Promise<RecordedResponse>; dispose?: () => void } | null =
    null;
  let recordingUpstream: { close: () => Promise<void>; origin: string } | null = null;
  try {
    const shim = await import(shimEntrypoint);
    recordingUpstream = await startRecordingUpstream({
      adminOrigin: opts.adminOrigin,
      authHeaders: buildAdminAuthHeaders(opts.accessToken),
      calls,
    });
    proxy = shim.createDraftProxy({
      readMode: 'live-hybrid',
      port: 4000,
      shopifyAdminOrigin: recordingUpstream.origin,
      unsupportedMutationMode: 'passthrough',
    });

    const responsesByName = new Map<string, RecordedResponse>();

    const primary = loadDocumentVariablesAndHeaders(spec.proxyRequest, capture);
    const targetsWithOwnRequest: { target: SpecTarget; requestName: string }[] = [];
    for (const target of spec.comparison?.targets ?? []) {
      if (target.proxyRequest && target.proxyRequest.documentPath) {
        const requestName = target.name ?? `target-${targetsWithOwnRequest.length + 1}`;
        targetsWithOwnRequest.push({ target, requestName });
      }
    }

    const requestCount = (primary ? 1 : 0) + targetsWithOwnRequest.length;
    if (requestCount === 0) {
      throw new Error(`Spec ${spec.scenarioId} has no executable requests (no proxyRequest with documentPath).`);
    }

    log(`[parity-record] recording ${spec.scenarioId} (${requestCount} request(s))`);
    let primaryResponse: RecordedResponse | undefined;
    let previousResponse: RecordedResponse | undefined;
    if (primary) {
      const variables = substituteVariables(primary.variables, { capture, responsesByName }) as Record<string, unknown>;
      primaryResponse = (await proxy.processGraphQLRequest(
        {
          query: primary.document,
          variables,
        },
        { headers: primary.headers, apiVersion: primary.apiVersion },
      )) as RecordedResponse;
      responsesByName.set('primary', primaryResponse);
      previousResponse = primaryResponse;
      logRecordedResponse('primary', primaryResponse);
    }
    for (const { target, requestName } of targetsWithOwnRequest) {
      const loaded = loadDocumentVariablesAndHeaders(target.proxyRequest, capture);
      if (!loaded) continue;
      const variables = substituteVariables(loaded.variables, {
        capture,
        primaryResponse,
        previousResponse,
        responsesByName,
      }) as Record<string, unknown>;
      const response = (await proxy.processGraphQLRequest(
        {
          query: loaded.document,
          variables,
        },
        { headers: loaded.headers, apiVersion: loaded.apiVersion },
      )) as RecordedResponse;
      responsesByName.set(requestName, response);
      previousResponse = response;
      logRecordedResponse(requestName, response);
    }

    // Intentionally do NOT rewrite captured response data with the
    // proxy's response. The captured Shopify response is the source of
    // truth (recorded by the dedicated `scripts/capture-*.mts` scripts
    // against real Shopify). The parity assertion is `proxy.response ==
    // captured.response`. If we overwrote captured.response with the
    // proxy's current output, the assertion would become trivially
    // self-consistent and silently pass for broken proxy logic.
    //
    // If the captured Shopify state has drifted (Shopify returns
    // different data than what's in the fixture), re-run the
    // corresponding `capture-*.mts` script — that's the dedicated tool
    // for refreshing captured Shopify data. This recorder only writes
    // `upstreamCalls`.
    const responsesUsed = primary || targetsWithOwnRequest.length > 0;
    if (responsesUsed) {
      // No-op in this branch; preserve scope for symmetry / future
      // diagnostic logging.
    }

    rewriteCaptureNow = () => rewriteCapture(captureFile, calls, []);
  } finally {
    proxy?.dispose?.();
    await recordingUpstream?.close();
  }

  if (rewriteCaptureNow) {
    rewriteCaptureNow();
  } else {
    rewriteCapture(captureFile, calls, []);
  }
  log(`[parity-record] wrote ${calls.length} upstreamCalls to ${relative(repoRoot, captureFile)}`);
}

function logRecordedResponse(name: string, response: RecordedResponse): void {
  if (response.status >= 400) {
    const bodyPreview = JSON.stringify(response.body).slice(0, 200);
    log(`[parity-record]   ${name}: status=${response.status} body=${bodyPreview}`);
  } else {
    log(`[parity-record]   ${name}: status=${response.status}`);
  }
}

async function main(): Promise<void> {
  const argv = process.argv.slice(2);
  let parsed: ReturnType<typeof parseArgs>;
  try {
    parsed = parseArgs(argv);
  } catch (err) {
    logError((err as Error).message);
    logError('Usage: pnpm parity:record <scenario-id> | --spec <path> | --all');
    process.exit(2);
    return;
  }

  if (!parsed.all && parsed.scenarioIds.length === 0 && parsed.specPaths.length === 0) {
    logError('Usage: pnpm parity:record <scenario-id> | --spec <path> | --all');
    process.exit(2);
    return;
  }

  const config = readConformanceScriptConfig({ exitOnMissing: true });
  const accessToken = await getValidConformanceAccessToken({
    adminOrigin: config.adminOrigin,
    apiVersion: config.apiVersion,
  });

  let specPaths: string[];
  if (parsed.all) {
    specPaths = await findAllSpecPaths();
  } else {
    specPaths = [];
    for (const id of parsed.scenarioIds) {
      specPaths.push(await findSpecForScenario(id));
    }
    for (const path of parsed.specPaths) {
      specPaths.push(isAbsolute(path) ? path : resolve(repoRoot, path));
    }
  }

  let failures = 0;
  for (const specPath of specPaths) {
    try {
      await recordSpec({
        specPath,
        adminOrigin: config.adminOrigin,
        apiVersion: config.apiVersion,
        accessToken,
      });
    } catch (err) {
      failures++;
      logError(`[parity-record] FAILED ${relative(repoRoot, specPath)}: ${(err as Error).message}`);
      if (!parsed.all) throw err;
    }
  }

  if (failures > 0) {
    logError(`[parity-record] ${failures}/${specPaths.length} spec(s) failed`);
    process.exit(1);
  }
}

await main();
