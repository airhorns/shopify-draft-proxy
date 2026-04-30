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

import * as gleam from '../../build/dev/javascript/shopify_draft_proxy/shopify_draft_proxy/proxy/draft_proxy.mjs';
import * as gleamOption from '../../build/dev/javascript/gleam_stdlib/gleam/option.mjs';
import * as gleamJson from '../../build/dev/javascript/gleam_json/gleam/json.mjs';
import * as gleamDict from '../../build/dev/javascript/gleam_stdlib/gleam/dict.mjs';
import { List as GleamList } from '../../build/dev/javascript/prelude.mjs';

import type {
  AppConfig,
  DraftProxyConfigSnapshot,
  DraftProxyHeaderValue,
  DraftProxyHttpResponse,
  DraftProxyLogSnapshot,
  DraftProxyRequest,
  DraftProxyStateDump,
  DraftProxyStateSnapshot,
  ReadMode,
} from './types.js';

type GleamProxy = unknown;

function readModeToGleam(mode: ReadMode): unknown {
  switch (mode) {
    case 'snapshot':
      return new (gleam as any).Snapshot();
    case 'live-hybrid':
      return new (gleam as any).LiveHybrid();
    case 'live':
      return new (gleam as any).Live();
  }
}

function snapshotPathOption(path: string | undefined): unknown {
  return path === undefined ? new (gleamOption as any).None() : new (gleamOption as any).Some(path);
}

function configToGleam(config: AppConfig): unknown {
  return new (gleam as any).Config(
    readModeToGleam(config.readMode),
    config.port,
    config.shopifyAdminOrigin,
    snapshotPathOption(config.snapshotPath),
  );
}

function headersToDict(headers: Record<string, DraftProxyHeaderValue> | undefined): unknown {
  let dict = (gleamDict as any).new$();
  if (!headers) return dict;
  for (const [key, value] of Object.entries(headers)) {
    if (value === undefined) continue;
    const flat = Array.isArray(value) ? value.join(',') : value;
    dict = (gleamDict as any).insert(dict, key, flat);
  }
  return dict;
}

function bodyToString(body: unknown): string {
  if (body === undefined || body === null) return '';
  if (typeof body === 'string') return body;
  return JSON.stringify(body);
}

function requestToGleam(request: DraftProxyRequest): unknown {
  return new (gleam as any).Request(
    request.method,
    request.path,
    headersToDict(request.headers),
    bodyToString(request.body),
  );
}

function responseFromGleam(resp: any): DraftProxyHttpResponse {
  const headers = gleamHeaderListToObject(resp.headers);
  const out: DraftProxyHttpResponse = {
    status: resp.status,
    body: JSON.parse((gleamJson as any).to_string(resp.body)),
  };
  if (headers !== undefined) out.headers = headers;
  return out;
}

function gleamHeaderListToObject(headers: GleamList<[string, string]> | undefined): Record<string, string> | undefined {
  if (!headers) return undefined;
  const out: Record<string, string> = {};
  // Gleam Lists are linked; walk via toArray for simplicity.
  const arr = (headers as any).toArray() as Array<[string, string]>;
  for (const [k, v] of arr) {
    out[k] = v;
  }
  return Object.keys(out).length > 0 ? out : undefined;
}

function unwrapPair(pair: any): [unknown, GleamProxy] {
  // Gleam tuples emit as JS arrays.
  return [pair[0], pair[1]];
}

export class DraftProxy {
  #inner: GleamProxy;

  constructor(config: AppConfig) {
    let proxy = (gleam as any).with_config(configToGleam(config));
    proxy = (gleam as any).with_default_registry(proxy);
    this.#inner = proxy;
  }

  static fromExisting(inner: GleamProxy): DraftProxy {
    const proxy = Object.create(DraftProxy.prototype) as DraftProxy;
    (proxy as any).constructor = DraftProxy;
    (proxy as any)['#inner'] = inner;
    return proxy;
  }

  async processRequest(request: DraftProxyRequest): Promise<DraftProxyHttpResponse> {
    const result = await Promise.resolve((gleam as any).process_request_async(this.#inner, requestToGleam(request)));
    const [resp, next] = unwrapPair(result);
    this.#inner = next;
    return responseFromGleam(resp);
  }

  reset(): void {
    this.#inner = (gleam as any).reset(this.#inner);
  }

  getConfig(): DraftProxyConfigSnapshot {
    return JSON.parse((gleamJson as any).to_string((gleam as any).get_config_snapshot(this.#inner)));
  }

  getLog(): DraftProxyLogSnapshot {
    return JSON.parse((gleamJson as any).to_string((gleam as any).get_log_snapshot(this.#inner)));
  }

  getState(): DraftProxyStateSnapshot {
    return JSON.parse((gleamJson as any).to_string((gleam as any).get_state_snapshot(this.#inner)));
  }

  dumpState(createdAt?: string): DraftProxyStateDump {
    const tree =
      createdAt === undefined
        ? (gleam as any).dump_state_now(this.#inner)
        : (gleam as any).dump_state(this.#inner, createdAt);
    return JSON.parse((gleamJson as any).to_string(tree));
  }

  restoreState(dump: DraftProxyStateDump): void {
    const result = (gleam as any).restore_state(this.#inner, JSON.stringify(dump));
    if (result.constructor.name === 'Error') {
      throw new Error(`DraftProxy.restoreState failed: ${result[0]?.message ?? 'malformed dump'}`);
    }
    this.#inner = result[0];
  }
}

export function createDraftProxy(config: AppConfig): DraftProxy {
  return new DraftProxy(config);
}
