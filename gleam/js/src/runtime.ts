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

import { readFileSync } from 'node:fs';

import {
  Config,
  type DraftProxy as GleamDraftProxy,
  GraphQLRequestOptions as GleamGraphQLRequestOptions,
  Live,
  LiveHybrid,
  Request as GleamRequest,
  type Response as GleamResponse,
  commit as gleamCommit,
  Snapshot,
  dump_state,
  dump_state_now,
  get_config_snapshot,
  get_log_snapshot,
  get_state_snapshot,
  process_graphql_request_async,
  process_request_async,
  reset as gleamReset,
  restore_snapshot,
  restore_state,
  with_config,
  with_default_registry,
} from '../../build/dev/javascript/shopify_draft_proxy/shopify_draft_proxy/proxy/draft_proxy.mjs';
import { None, Some } from '../../build/dev/javascript/gleam_stdlib/gleam/option.mjs';
import { to_string as jsonToString } from '../../build/dev/javascript/gleam_json/gleam/json.mjs';
import { insert as dictInsert, new$ as dictNew } from '../../build/dev/javascript/gleam_stdlib/gleam/dict.mjs';
import {
  Result$Error$0,
  Result$isOk,
  Result$Ok$0,
  type List,
  type Result,
} from '../../build/dev/javascript/prelude.mjs';

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

  constructor(options: DraftProxyOptions) {
    this.#inner = with_default_registry(with_config(configToGleam(options)));
    if (options.snapshotPath !== undefined) {
      this.#inner = unwrapProxyResult(
        restore_snapshot(this.#inner, readFileSync(options.snapshotPath, 'utf8')),
        'snapshot loading',
      );
    }
    if (options.state !== undefined) {
      this.restoreState(options.state);
    }
  }

  async processRequest(request: DraftProxyRequest): Promise<DraftProxyHttpResponse> {
    const [resp, next] = await process_request_async(this.#inner, requestToGleam(request));
    this.#inner = next;
    return responseFromGleam(resp);
  }

  async processGraphQLRequest(
    body: unknown,
    options: DraftProxyGraphQLRequestOptions = {},
  ): Promise<DraftProxyHttpResponse> {
    const gleamOptions = new GleamGraphQLRequestOptions(
      options.path === undefined ? new None() : new Some(options.path),
      options.apiVersion === undefined ? new None() : new Some(options.apiVersion),
      headersToDict(options.headers),
    );
    const [resp, next] = await process_graphql_request_async(this.#inner, bodyToString(body), gleamOptions);
    this.#inner = next;
    return responseFromGleam(resp);
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
    this.#inner = unwrapProxyResult(restore_state(this.#inner, JSON.stringify(dump)), 'restoreState');
  }

  async commit(headers: Record<string, DraftProxyHeaderValue> = {}): Promise<DraftProxyCommitResult> {
    const [resp, next] = await gleamCommit(this.#inner, headersToDict(headers));
    this.#inner = next;
    const body = responseFromGleam(resp).body as {
      ok?: boolean;
      stopIndex?: number | null;
      attempts?: DraftProxyCommitResult['attempts'];
    };
    const result = {
      ok: Boolean(body.ok),
      stopIndex: body.stopIndex ?? null,
      attempts: body.attempts ?? [],
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

function unwrapProxyResult(result: Result<GleamDraftProxy, unknown>, action: string): GleamDraftProxy {
  if (Result$isOk(result)) {
    return Result$Ok$0(result) as GleamDraftProxy;
  }
  const err = Result$Error$0(result);
  const message =
    err && typeof err === 'object' && 'message' in err
      ? String((err as { message: unknown }).message)
      : 'malformed dump';
  throw new Error(`DraftProxy.${action} failed: ${message}`);
}

export function createDraftProxy(options: DraftProxyOptions): DraftProxy {
  return new DraftProxy(options);
}
