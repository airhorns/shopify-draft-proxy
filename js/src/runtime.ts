import { spawn, spawnSync, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import type {
  AppConfig,
  DraftProxyCommitAttempt,
  DraftProxyCommitResult,
  DraftProxyConfigSnapshot,
  DraftProxyGraphQLRequestOptions,
  DraftProxyHeaderValue,
  DraftProxyHttpResponse,
  DraftProxyLogSnapshot,
  DraftProxyOptions,
  DraftProxyRequest,
  DraftProxyStateDump,
  DraftProxyStateSnapshot,
} from './types.js';
import { DraftProxyCommitError } from './types.js';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');
const activeChildren = new Set<ChildProcessWithoutNullStreams>();
let cleanupRegistered = false;

function registerCleanup(): void {
  if (cleanupRegistered) return;
  cleanupRegistered = true;
  process.once('exit', () => {
    for (const child of activeChildren) {
      child.kill();
    }
  });
}

function allocatePort(): number {
  const script = `
    const net = require('node:net');
    const server = net.createServer();
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      console.log(address.port);
      server.close();
    });
  `;
  const result = spawnSync(process.execPath, ['-e', script], { encoding: 'utf8' });
  const port = Number(result.stdout.trim());
  if (!Number.isInteger(port) || port <= 0) {
    throw new Error(`Failed to allocate port for Rust DraftProxy runtime: ${result.stderr}`);
  }
  return port;
}

function sleepSync(ms: number): void {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function bodyToString(body: unknown): string {
  if (body === undefined || body === null) return '';
  if (typeof body === 'string') return body;
  return JSON.stringify(body);
}

function normalizeHeaders(headers: Record<string, DraftProxyHeaderValue> | undefined): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(headers ?? {})) {
    if (value === undefined) continue;
    out[key] = Array.isArray(value) ? value.join(',') : value;
  }
  return out;
}

function responseHeaders(headers: Headers): Record<string, string> | undefined {
  const out: Record<string, string> = {};
  headers.forEach((value, key) => {
    out[key] = value;
  });
  return Object.keys(out).length > 0 ? out : undefined;
}

function bulkOperationResultPath(operationIdOrUrl: string): string {
  if (operationIdOrUrl.startsWith('http://') || operationIdOrUrl.startsWith('https://')) {
    return new URL(operationIdOrUrl).pathname;
  }
  const tail = operationIdOrUrl.split('/').pop()?.split('?')[0] ?? operationIdOrUrl;
  return `/__meta/bulk-operations/${encodeURIComponent(tail)}/result.jsonl`;
}

async function fetchJson(origin: string, request: DraftProxyRequest): Promise<DraftProxyHttpResponse> {
  const headers = normalizeHeaders(request.headers);
  const body = bodyToString(request.body);
  const init: RequestInit = {
    method: request.method,
    headers,
  };
  if (body.length > 0) init.body = body;
  const response = await fetch(`${origin}${request.path}`, init);
  const text = await response.text();
  let parsed: unknown = text;
  if (text.length > 0) {
    try {
      parsed = JSON.parse(text);
    } catch {
      parsed = text;
    }
  }
  const out: DraftProxyHttpResponse = { status: response.status, body: parsed };
  const headersObject = responseHeaders(response.headers);
  if (headersObject !== undefined) out.headers = headersObject;
  return out;
}

function fetchJsonSync(origin: string, request: DraftProxyRequest, timeoutMs = 10_000): DraftProxyHttpResponse {
  // Pass the request via stdin rather than an environment variable to avoid
  // E2BIG failures when the body is large (e.g. dumpState/restoreState with
  // hundreds of staged variants).
  const script = `
    const fs = require('node:fs');
    const request = JSON.parse(fs.readFileSync(0, 'utf8'));
    const timeoutMs = Number(process.env.DRAFT_PROXY_FETCH_TIMEOUT_MS || 10000);
    const signal = AbortSignal.timeout(timeoutMs);
    fetch(process.env.DRAFT_PROXY_URL + request.path, {
      method: request.method,
      headers: request.headers,
      body: request.body.length === 0 ? undefined : request.body,
      signal,
    }).then(async (response) => {
      const text = await response.text();
      let body = text;
      if (text.length > 0) {
        try { body = JSON.parse(text); } catch {}
      }
      console.log(JSON.stringify({ status: response.status, headers: Object.fromEntries(response.headers.entries()), body }));
    }).catch((error) => {
      console.error(error && error.stack ? error.stack : String(error));
      process.exit(1);
    });
  `;
  const input = JSON.stringify({
    method: request.method,
    path: request.path,
    headers: normalizeHeaders(request.headers),
    body: bodyToString(request.body),
  });
  const result = spawnSync(process.execPath, ['-e', script], {
    input,
    encoding: 'utf8',
    env: {
      ...process.env,
      DRAFT_PROXY_URL: origin,
      DRAFT_PROXY_FETCH_TIMEOUT_MS: String(timeoutMs),
    },
    maxBuffer: 128 * 1024 * 1024,
    timeout: 10_000,
  });
  if (result.status !== 0) {
    throw new Error(`Rust DraftProxy sync request failed: ${result.error?.message || result.stderr || result.stdout}`);
  }
  return JSON.parse(result.stdout) as DraftProxyHttpResponse;
}

function waitForRustServer(child: ChildProcessWithoutNullStreams, origin: string, output: () => string): void {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    if (output().includes('shopify-draft-proxy rust runtime listening')) return;
    if (child.exitCode !== null) {
      throw new Error(`Rust DraftProxy runtime exited before listening:\n${output()}`);
    }
    try {
      const response = fetchJsonSync(origin, { method: 'GET', path: '/__meta/health' }, 250);
      if (response.status === 200) return;
    } catch {
      // Server is not accepting connections yet.
    }
    sleepSync(100);
  }
  throw new Error(`Rust DraftProxy runtime did not start before timeout:\n${output()}`);
}

function envForConfig(config: AppConfig, port: number): NodeJS.ProcessEnv {
  return {
    ...process.env,
    PORT: String(port),
    SHOPIFY_ADMIN_ORIGIN: config.shopifyAdminOrigin,
    READ_MODE: config.readMode,
    UNSUPPORTED_MUTATION_MODE: config.unsupportedMutationMode ?? 'passthrough',
    BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES: String(
      config.bulkOperationRunMutationMaxInputFileSizeBytes ?? 104_857_600,
    ),
    ...(config.snapshotPath === undefined ? {} : { SNAPSHOT_PATH: config.snapshotPath }),
  };
}

export class DraftProxy {
  readonly #origin: string;
  readonly #child: ChildProcessWithoutNullStreams;

  constructor(options: DraftProxyOptions) {
    const port = allocatePort();
    this.#origin = `http://127.0.0.1:${port}`;
    registerCleanup();
    this.#child = spawn('./target/release/shopify-draft-proxy-server', [], {
      cwd: repoRoot,
      env: envForConfig(options, port),
    });
    activeChildren.add(this.#child);
    this.#child.on('exit', () => {
      activeChildren.delete(this.#child);
    });
    let output = '';
    this.#child.stdout.on('data', (chunk: Buffer) => {
      output += chunk.toString();
    });
    this.#child.stderr.on('data', (chunk: Buffer) => {
      output += chunk.toString();
    });
    waitForRustServer(this.#child, this.#origin, () => output);

    if (options.state !== undefined) {
      this.restoreState(options.state);
    }
  }

  dispose(): void {
    activeChildren.delete(this.#child);
    this.#child.kill();
  }

  async processRequest(request: DraftProxyRequest): Promise<DraftProxyHttpResponse> {
    return await fetchJson(this.#origin, request);
  }

  stageStagedUpload(encodedTargetId: string, encodedFilename: string, content: string): { ok: true; key: string } {
    const response = fetchJsonSync(this.#origin, {
      method: 'PUT',
      path: `/staged-uploads/${encodedTargetId}/${encodedFilename}`,
      body: content,
    });
    if (response.status !== 201) {
      throw new Error(`DraftProxy.stageStagedUpload failed with status ${response.status}`);
    }
    return response.body as { ok: true; key: string };
  }

  getBulkOperationResultJsonl(_operationId: string): string | null {
    const response = fetchJsonSync(this.#origin, {
      method: 'GET',
      path: bulkOperationResultPath(_operationId),
    });
    if (response.status === 404) return null;
    if (response.status !== 200) {
      throw new Error(`DraftProxy.getBulkOperationResultJsonl failed with status ${response.status}`);
    }
    if (typeof response.body === 'string') return response.body;
    return JSON.stringify(response.body);
  }

  async processGraphQLRequest(
    body: unknown,
    options: DraftProxyGraphQLRequestOptions = {},
  ): Promise<DraftProxyHttpResponse> {
    const path = options.path ?? `/admin/api/${options.apiVersion ?? '2025-01'}/graphql.json`;
    return await this.processRequest({
      method: 'POST',
      path,
      headers: { 'content-type': 'application/json', ...normalizeHeaders(options.headers) },
      body,
    });
  }

  installSyncTransport(_send: unknown): void {
    throw new Error('DraftProxy.installSyncTransport is not available on the Rust HTTP runtime shim.');
  }

  reset(): void {
    fetchJsonSync(this.#origin, { method: 'POST', path: '/__meta/reset' });
  }

  getConfig(): DraftProxyConfigSnapshot {
    return fetchJsonSync(this.#origin, { method: 'GET', path: '/__meta/config' }).body as DraftProxyConfigSnapshot;
  }

  getLog(): DraftProxyLogSnapshot {
    return fetchJsonSync(this.#origin, { method: 'GET', path: '/__meta/log' }).body as DraftProxyLogSnapshot;
  }

  getState(): DraftProxyStateSnapshot {
    return fetchJsonSync(this.#origin, { method: 'GET', path: '/__meta/state' }).body as DraftProxyStateSnapshot;
  }

  dumpState(createdAt?: string): DraftProxyStateDump {
    return fetchJsonSync(this.#origin, {
      method: 'POST',
      path: '/__meta/dump',
      headers: { 'content-type': 'application/json' },
      body: { createdAt },
    }).body as DraftProxyStateDump;
  }

  restoreState(dump: DraftProxyStateDump): void {
    const response = fetchJsonSync(this.#origin, {
      method: 'POST',
      path: '/__meta/restore',
      headers: { 'content-type': 'application/json' },
      body: dump,
    });
    if (response.status !== 200) {
      throw new Error(`DraftProxy.restoreState failed with status ${response.status}`);
    }
  }

  async commit(headers: Record<string, DraftProxyHeaderValue> = {}): Promise<DraftProxyCommitResult> {
    const response = await this.processRequest({ method: 'POST', path: '/__meta/commit', headers });
    const body = response.body as { ok?: boolean; committed?: number; failed?: number };
    const log = this.getLog() as { entries?: Array<Record<string, unknown>> };
    const attempts: DraftProxyCommitAttempt[] = (log.entries ?? []).map((entry) => ({
      logEntryId: String(entry['id'] ?? ''),
      operationName: (entry['operationName'] ?? null) as string | null,
      path: String(entry['path'] ?? ''),
      success: entry['status'] === 'committed',
      status: String(entry['status'] ?? ''),
      upstreamStatus: null,
      upstreamBody: null,
      upstreamError: null,
      responseBody: null,
    }));
    if (!body.ok) {
      throw new DraftProxyCommitError({ ok: false, stopIndex: body.failed ?? null, attempts });
    }
    return { stopIndex: null, attempts };
  }
}

export function createDraftProxy(options: DraftProxyOptions): DraftProxy {
  return new DraftProxy(options);
}
