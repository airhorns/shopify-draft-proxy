// TS-side public types for the Gleam shim. Every exported symbol from
// the package runtime surface should stay callable through this shim
// with the same shape.

export type ReadMode = 'snapshot' | 'live-hybrid' | 'passthrough';

export interface AppConfig {
  readMode: ReadMode;
  port: number;
  shopifyAdminOrigin: string;
  snapshotPath?: string;
}

export type DraftProxyHeaderValue = string | string[] | undefined;

export interface DraftProxyRequest {
  method: string;
  path: string;
  headers?: Record<string, DraftProxyHeaderValue>;
  body?: unknown;
}

export interface DraftProxyHttpResponse {
  status: number;
  body: unknown;
  headers?: Record<string, string>;
}

export interface DraftProxyGraphQLRequestOptions {
  path?: string;
  apiVersion?: string;
  headers?: Record<string, DraftProxyHeaderValue>;
}

export interface DraftProxyOptions extends AppConfig {
  state?: DraftProxyStateDump;
}

export interface DraftProxyConfigSnapshot {
  runtime: { readMode: ReadMode };
  proxy: { port: number; shopifyAdminOrigin: string };
  snapshot: { enabled: boolean; path: string | null };
}

export type DraftProxyLogSnapshot = unknown;
export type DraftProxyStateSnapshot = unknown;

export const DRAFT_PROXY_STATE_DUMP_SCHEMA = 'shopify-draft-proxy/state-dump';

export interface DraftProxyStateDump {
  schema: typeof DRAFT_PROXY_STATE_DUMP_SCHEMA;
  version: 1;
  createdAt: string;
  store: unknown;
  syntheticIdentity: unknown;
  extensions?: Record<string, unknown>;
}

export interface DraftProxyCommitAttempt {
  logEntryId: string;
  operationName: string | null;
  path: string;
  success: boolean;
  status: string;
  upstreamStatus: number | null;
  upstreamBody: unknown;
  upstreamError: { message: string } | null;
  responseBody: unknown;
}

export interface DraftProxyCommitResult {
  stopIndex: null;
  attempts: DraftProxyCommitAttempt[];
}

export class DraftProxyCommitError extends Error {
  readonly result: { ok: boolean; stopIndex: number | null; attempts: DraftProxyCommitAttempt[] };

  constructor(result: { ok: boolean; stopIndex: number | null; attempts: DraftProxyCommitAttempt[] }) {
    super('DraftProxy commit failed before all staged mutations were replayed.');
    this.name = 'DraftProxyCommitError';
    this.result = result;
  }
}
