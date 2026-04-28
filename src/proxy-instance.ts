import type { AppConfig } from './config.js';
import {
  commitMetaState,
  getMetaConfig,
  getMetaHealth,
  getMetaLog,
  renderMetaWebUi,
  resetMetaState,
  type MetaCommitResponse,
  type MetaHealthResponse,
  type MetaResetResponse,
} from './meta/routes.js';
import { processProxyGraphQLRequest } from './proxy/routes.js';
import { loadNormalizedStateSnapshot } from './state/snapshot-loader.js';
import { getDefaultStore, InMemoryStore, runWithStore } from './state/store.js';
import {
  getDefaultSyntheticIdentity,
  runWithSyntheticIdentity,
  SyntheticIdentityRegistry,
} from './state/synthetic-identity.js';

export type DraftProxyHeaderValue = string | string[] | undefined;

export interface DraftProxyRequest {
  method: string;
  path: string;
  headers?: Record<string, DraftProxyHeaderValue>;
  body?: unknown;
}

export interface DraftProxyResponse {
  status: number;
  body: unknown;
  headers?: Record<string, string>;
}

export interface DraftProxyGraphQLRequestOptions {
  path?: string;
  apiVersion?: string;
  headers?: Record<string, DraftProxyHeaderValue>;
}

export interface DraftProxyOptions {
  store?: InMemoryStore;
  syntheticIdentity?: SyntheticIdentityRegistry;
}

export type DraftProxyConfigResponse = ReturnType<typeof getMetaConfig>;
export type DraftProxyLogResponse = ReturnType<typeof getMetaLog>;
export type DraftProxyStateResponse = ReturnType<InMemoryStore['getState']>;

const ADMIN_GRAPHQL_ROUTE_PATTERN = /^\/admin\/api\/[^/]+\/graphql\.json$/u;
const BULK_OPERATION_RESULT_ROUTE_PATTERN = /^\/__bulk_operations\/([^/]+)\/result\.jsonl$/u;
const META_BULK_OPERATION_RESULT_ROUTE_PATTERN = /^\/__meta\/bulk-operations\/(.+)\/result\.jsonl$/u;
const STAGED_UPLOAD_ROUTE_PATTERN = /^\/staged-uploads\/([^/]+)\/(.+)$/u;

function defaultGraphQLPath(apiVersion: string | undefined): string {
  return `/admin/api/${apiVersion ?? '2025-01'}/graphql.json`;
}

function methodIs(input: DraftProxyRequest, method: string): boolean {
  return input.method.toUpperCase() === method;
}

function methodNotAllowed(): DraftProxyResponse {
  return {
    status: 405,
    body: { errors: [{ message: 'Method not allowed' }] },
  };
}

function withOptionalHeaders<T extends { headers?: Record<string, DraftProxyHeaderValue> }>(
  target: Omit<T, 'headers'>,
  headers: Record<string, DraftProxyHeaderValue> | undefined,
): T {
  return {
    ...target,
    ...(headers ? { headers } : {}),
  } as T;
}

function requestBodyToText(body: unknown): string {
  if (typeof body === 'string') {
    return body;
  }
  if (Buffer.isBuffer(body)) {
    return body.toString('utf8');
  }
  if (body && typeof body === 'object') {
    return JSON.stringify(body);
  }
  return '';
}

export class DraftProxy {
  readonly config: AppConfig;

  private readonly runtimeStore: InMemoryStore;
  private readonly syntheticIdentity: SyntheticIdentityRegistry;

  constructor(config: AppConfig, options: DraftProxyOptions = {}) {
    this.config = config;
    this.runtimeStore = options.store ?? new InMemoryStore();
    this.syntheticIdentity = options.syntheticIdentity ?? new SyntheticIdentityRegistry();

    if (config.snapshotPath) {
      this.runtimeStore.installSnapshot(loadNormalizedStateSnapshot(config.snapshotPath));
    }
  }

  withRuntimeContext<T>(callback: () => T): T {
    return runWithStore(this.runtimeStore, () => runWithSyntheticIdentity(this.syntheticIdentity, callback));
  }

  async processRequest(input: DraftProxyRequest): Promise<DraftProxyResponse> {
    if (input.path === '/__meta') {
      if (!methodIs(input, 'GET')) {
        return methodNotAllowed();
      }
      return {
        status: 200,
        headers: { 'content-type': 'text/html; charset=utf-8' },
        body: renderMetaWebUi(this.config, this.runtimeStore),
      };
    }

    if (input.path === '/__meta/health') {
      if (!methodIs(input, 'GET')) {
        return methodNotAllowed();
      }
      return { status: 200, body: this.health() };
    }

    if (input.path === '/__meta/config') {
      if (!methodIs(input, 'GET')) {
        return methodNotAllowed();
      }
      return { status: 200, body: this.getConfig() };
    }

    if (input.path === '/__meta/log') {
      if (!methodIs(input, 'GET')) {
        return methodNotAllowed();
      }
      return { status: 200, body: this.getLog() };
    }

    if (input.path === '/__meta/state') {
      if (!methodIs(input, 'GET')) {
        return methodNotAllowed();
      }
      return { status: 200, body: this.getState() };
    }

    const metaBulkOperationResultMatch = META_BULK_OPERATION_RESULT_ROUTE_PATTERN.exec(input.path);
    if (metaBulkOperationResultMatch) {
      if (!methodIs(input, 'GET')) {
        return methodNotAllowed();
      }

      const encodedOperationId = metaBulkOperationResultMatch[1];
      if (!encodedOperationId) {
        return {
          status: 404,
          body: 'Bulk operation result not found',
        };
      }

      const operationId = decodeURIComponent(encodedOperationId);
      const operation = this.runtimeStore.getEffectiveBulkOperationById(operationId);

      if (!operation?.resultJsonl) {
        return {
          status: 404,
          body: 'Bulk operation result not found',
        };
      }

      return {
        status: 200,
        headers: { 'content-type': 'application/jsonl; charset=utf-8' },
        body: operation.resultJsonl,
      };
    }

    if (input.path === '/__meta/reset') {
      if (!methodIs(input, 'POST')) {
        return methodNotAllowed();
      }
      return { status: 200, body: this.reset() };
    }

    if (input.path === '/__meta/commit') {
      if (!methodIs(input, 'POST')) {
        return methodNotAllowed();
      }
      return { status: 200, body: await this.commit(input.headers ?? {}) };
    }

    const bulkOperationResultMatch = BULK_OPERATION_RESULT_ROUTE_PATTERN.exec(input.path);
    if (bulkOperationResultMatch) {
      if (!methodIs(input, 'GET')) {
        return methodNotAllowed();
      }

      const jsonl = this.runtimeStore.getEffectiveBulkOperationResultJsonl(
        `gid://shopify/BulkOperation/${bulkOperationResultMatch[1]}`,
      );

      if (jsonl === null) {
        return {
          status: 404,
          body: 'Bulk operation result not found',
        };
      }

      return {
        status: 200,
        headers: { 'content-type': 'application/jsonl; charset=utf-8' },
        body: jsonl,
      };
    }

    const stagedUploadMatch = STAGED_UPLOAD_ROUTE_PATTERN.exec(input.path);
    if (stagedUploadMatch) {
      if (!methodIs(input, 'POST') && !methodIs(input, 'PUT')) {
        return methodNotAllowed();
      }

      const encodedTargetId = stagedUploadMatch[1];
      const encodedFilenameFromPath = stagedUploadMatch[2];
      if (!encodedTargetId || !encodedFilenameFromPath) {
        return {
          status: 404,
          body: { errors: [{ message: 'Not found' }] },
        };
      }

      const targetId = decodeURIComponent(encodedTargetId);
      const filename = decodeURIComponent(encodedFilenameFromPath);
      const key = `shopify-draft-proxy/${targetId}/${filename}`;
      const encodedId = encodeURIComponent(targetId);
      const encodedFilename = encodeURIComponent(filename);

      this.runtimeStore.stageUploadContent(
        [
          key,
          `/staged-uploads/${encodedId}/${encodedFilename}`,
          `https://shopify-draft-proxy.local/staged-uploads/${encodedId}/${encodedFilename}`,
        ],
        requestBodyToText(input.body),
      );

      return {
        status: 201,
        body: { ok: true, key },
      };
    }

    return this.withRuntimeContext(async () => {
      if (ADMIN_GRAPHQL_ROUTE_PATTERN.test(input.path)) {
        if (!methodIs(input, 'POST')) {
          return methodNotAllowed();
        }
        return processProxyGraphQLRequest(
          this.config,
          withOptionalHeaders(
            {
              path: input.path,
              body: input.body,
            },
            input.headers,
          ),
        );
      }

      return {
        status: 404,
        body: { errors: [{ message: 'Not found' }] },
      };
    });
  }

  processGraphQLRequest(body: unknown, options: DraftProxyGraphQLRequestOptions = {}): Promise<DraftProxyResponse> {
    return this.processRequest(
      withOptionalHeaders(
        {
          method: 'POST',
          path: options.path ?? defaultGraphQLPath(options.apiVersion),
          body,
        },
        options.headers,
      ),
    );
  }

  health(): MetaHealthResponse {
    return this.withRuntimeContext(() => getMetaHealth());
  }

  getConfig(): DraftProxyConfigResponse {
    return getMetaConfig(this.config);
  }

  getLog(): DraftProxyLogResponse {
    return getMetaLog(this.runtimeStore);
  }

  getState(): DraftProxyStateResponse {
    return this.runtimeStore.getState();
  }

  reset(): MetaResetResponse {
    return resetMetaState(this.runtimeStore, this.syntheticIdentity);
  }

  clear(): MetaResetResponse {
    return this.reset();
  }

  commit(headers: Record<string, DraftProxyHeaderValue> = {}): Promise<MetaCommitResponse> {
    return commitMetaState(
      this.config,
      {
        path: '/__meta/commit',
        request: { headers },
      },
      this.runtimeStore,
    );
  }

  flush(headers: Record<string, DraftProxyHeaderValue> = {}): Promise<MetaCommitResponse> {
    return this.commit(headers);
  }
}

export function createDraftProxy(config: AppConfig, options?: DraftProxyOptions): DraftProxy {
  return new DraftProxy(config, options);
}

export function createDefaultStoreDraftProxy(config: AppConfig): DraftProxy {
  return createDraftProxy(config, {
    store: getDefaultStore(),
    syntheticIdentity: getDefaultSyntheticIdentity(),
  });
}
