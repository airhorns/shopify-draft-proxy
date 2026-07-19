// TS-side public types for the Rust runtime shim. Every exported symbol from
// the package runtime surface should stay callable through this shim
// with the same shape.

export type ReadMode = 'snapshot' | 'live-hybrid' | 'passthrough';
export type UnsupportedMutationMode = 'passthrough' | 'reject';

export interface AppConfig {
  readMode: ReadMode;
  port: number;
  shopifyAdminOrigin: string;
  snapshotPath?: string;
  unsupportedMutationMode?: UnsupportedMutationMode;
  bulkOperationRunMutationMaxInputFileSizeBytes?: number;
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
  runtime: {
    readMode: ReadMode;
    unsupportedMutationMode: UnsupportedMutationMode;
    bulkOperationRunMutationMaxInputFileSizeBytes: number;
  };
  proxy: { port: number; shopifyAdminOrigin: string };
  snapshot: { enabled: boolean; path: string | null };
}

export type DraftProxyLogSnapshot = unknown;
export type DraftProxyStateSnapshot = unknown;

export const DRAFT_PROXY_STATE_DUMP_SCHEMA = 'shopify-draft-proxy-rust-state/v1';

export interface DraftProxyStateDump {
  schema: typeof DRAFT_PROXY_STATE_DUMP_SCHEMA;
  version: 1;
  createdAt: string;
  store: unknown;
  syntheticIdentity: unknown;
  extensions?: Record<string, unknown>;
}

export interface DraftProxyCommitAttemptRequest {
  method: string;
  path: string;
}

export interface DraftProxyCommitAttemptResponse {
  status: number;
  body: unknown;
}

export interface DraftProxyUnresolvedIdMapping {
  syntheticId: string;
  operation: string | null;
  responsePath: string[] | null;
  reason: string;
}

export interface DraftProxyCommitAttempt {
  index: number;
  logId: string;
  status: string;
  request: DraftProxyCommitAttemptRequest;
  response: DraftProxyCommitAttemptResponse;
  mappedIds?: Record<string, string>;
  unresolvedIds?: DraftProxyUnresolvedIdMapping[];
  error?: string;
}

export interface DraftProxyCommitResult {
  ok: boolean;
  committed: number;
  failed: number;
  stopIndex: number | null;
  attempts: DraftProxyCommitAttempt[];
  error?: string;
}

export class DraftProxyCommitError extends Error {
  readonly result: DraftProxyCommitResult;

  constructor(result: DraftProxyCommitResult) {
    super('DraftProxy commit failed before all staged mutations were replayed.');
    this.name = 'DraftProxyCommitError';
    this.result = result;
  }
}
