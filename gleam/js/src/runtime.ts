// Bridge between TS public API and the Gleam-emitted ESM. Translates
// JS-shaped requests / config into Gleam record instances and unwraps
// Gleam tuples back into TS-shaped responses.
//
// Path note: the Gleam build tree is `gleam/build/dev/javascript/...`
// and this shim lives at `gleam/js/src/`. Three `..` jumps land at
// `gleam/build/...`. Once the shim is promoted to ship as a real
// package, the Gleam build can be relocated under `js/dist/` (or
// the relative import re-pointed) without changing this file's
// public surface.

import {
  Config,
  type DraftProxy as GleamDraftProxy,
  Live,
  LiveHybrid,
  Request as GleamRequest,
  type Response as GleamResponse,
  Snapshot,
  dump_state,
  dump_state_now,
  get_config_snapshot,
  get_log_snapshot,
  get_state_snapshot,
  process_request_async,
  reset as gleamReset,
  restore_state,
  with_config,
  with_default_registry,
} from '../../build/dev/javascript/shopify_draft_proxy/shopify_draft_proxy/proxy/draft_proxy.mjs';
import { None, Some } from '../../build/dev/javascript/gleam_stdlib/gleam/option.mjs';
import { to_string as jsonToString } from '../../build/dev/javascript/gleam_json/gleam/json.mjs';
import { insert as dictInsert, new$ as dictNew } from '../../build/dev/javascript/gleam_stdlib/gleam/dict.mjs';
import { Result$Error$0, Result$isOk, Result$Ok$0, type List } from '../../build/dev/javascript/prelude.mjs';

import type {
  AppConfig,
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
  ReadMode,
} from './types.js';
import { DraftProxyCommitError } from './types.js';

function readModeToGleam(mode: ReadMode): Snapshot | LiveHybrid | Live {
  switch (mode) {
    case 'snapshot':
      return new Snapshot();
    case 'live-hybrid':
      return new LiveHybrid();
    case 'live':
      return new Live();
  }
}

function configToGleam(config: AppConfig): Config {
  const snapshotPath = config.snapshotPath === undefined ? new None() : new Some(config.snapshotPath);
  return new Config(readModeToGleam(config.readMode), config.port, config.shopifyAdminOrigin, snapshotPath);
}

function headersToDict(headers: Record<string, DraftProxyHeaderValue> | undefined) {
  let dict = dictNew();
  if (!headers) return dict;
  for (const [key, value] of Object.entries(headers)) {
    if (value === undefined) continue;
    dict = dictInsert(dict, key, Array.isArray(value) ? value.join(',') : value);
  }
  return dict;
}

function bodyToString(body: unknown): string {
  if (body === undefined || body === null) return '';
  if (typeof body === 'string') return body;
  return JSON.stringify(body);
}

function requestToGleam(request: DraftProxyRequest): GleamRequest {
  return new GleamRequest(request.method, request.path, headersToDict(request.headers), bodyToString(request.body));
}

function headerListToObject(headers: List<[string, string]>): Record<string, string> | undefined {
  const out: Record<string, string> = {};
  for (const [k, v] of headers.toArray()) out[k] = v;
  return Object.keys(out).length > 0 ? out : undefined;
}

function responseFromGleam(resp: GleamResponse): DraftProxyHttpResponse {
  const headers = headerListToObject(resp.headers);
  const out: DraftProxyHttpResponse = {
    status: resp.status,
    body: JSON.parse(jsonToString(resp.body)),
  };
  if (headers !== undefined) out.headers = headers;
  return out;
}

export class DraftProxy {
  #inner: GleamDraftProxy;

  constructor(config: AppConfig, options: DraftProxyOptions = {}) {
    this.#inner = with_default_registry(with_config(configToGleam(config)));
    if (options.state !== undefined) this.restoreState(options.state);
  }

  async processRequest(request: DraftProxyRequest): Promise<DraftProxyHttpResponse> {
    const [resp, next] = await process_request_async(this.#inner, requestToGleam(request));
    this.#inner = next;
    return responseFromGleam(resp);
  }

  processGraphQLRequest(body: unknown, options: DraftProxyGraphQLRequestOptions = {}): Promise<DraftProxyHttpResponse> {
    return this.processRequest({
      method: 'POST',
      path: options.path ?? `/admin/api/${options.apiVersion ?? '2025-01'}/graphql.json`,
      headers: options.headers,
      body,
    });
  }

  reset(): void {
    this.#inner = gleamReset(this.#inner);
  }

  getConfig(): DraftProxyConfigSnapshot {
    return JSON.parse(jsonToString(get_config_snapshot(this.#inner)));
  }

  getLog(): DraftProxyLogSnapshot {
    return JSON.parse(jsonToString(get_log_snapshot(this.#inner)));
  }

  getState(): DraftProxyStateSnapshot {
    return JSON.parse(jsonToString(get_state_snapshot(this.#inner)));
  }

  dumpState(createdAt?: string): DraftProxyStateDump {
    const tree = createdAt === undefined ? dump_state_now(this.#inner) : dump_state(this.#inner, createdAt);
    return JSON.parse(jsonToString(tree));
  }

  restoreState(dump: DraftProxyStateDump): void {
    const result = restore_state(this.#inner, JSON.stringify(dump));
    if (Result$isOk(result)) {
      this.#inner = Result$Ok$0(result) as GleamDraftProxy;
      return;
    }
    const err = Result$Error$0(result);
    const message =
      err && typeof err === 'object' && 'message' in err
        ? String((err as { message: unknown }).message)
        : 'malformed dump';
    throw new Error(`DraftProxy.restoreState failed: ${message}`);
  }

  async commit(headers: Record<string, DraftProxyHeaderValue> = {}): Promise<DraftProxyCommitResult> {
    const response = await this.processRequest({
      method: 'POST',
      path: '/__meta/commit',
      headers,
    });
    const body = response.body as { ok?: boolean; stopIndex?: number | null; attempts?: unknown[] };
    const result = {
      ok: Boolean(body.ok),
      stopIndex: body.stopIndex ?? null,
      attempts: (body.attempts ?? []) as DraftProxyCommitResult['attempts'],
    };
    if (!result.ok) {
      throw new DraftProxyCommitError(result);
    }
    return {
      stopIndex: null,
      attempts: result.attempts,
    };
  }
}

export function createDraftProxy(config: AppConfig, options?: DraftProxyOptions): DraftProxy {
  return new DraftProxy(config, options);
}
