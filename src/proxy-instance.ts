import type { AppConfig } from './config.js';
import {
  commitMetaState,
  getMetaConfig,
  getMetaHealth,
  getMetaLog,
  getMetaState,
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
export type DraftProxyStateResponse = ReturnType<typeof getMetaState>;

const ADMIN_GRAPHQL_ROUTE_PATTERN = /^\/admin\/api\/[^/]+\/graphql\.json$/u;

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
    return this.withRuntimeContext(async () => {
      if (input.path === '/__meta') {
        if (!methodIs(input, 'GET')) {
          return methodNotAllowed();
        }
        return {
          status: 200,
          headers: { 'content-type': 'text/html; charset=utf-8' },
          body: renderMetaWebUi(this.config),
        };
      }

      if (input.path === '/__meta/health') {
        if (!methodIs(input, 'GET')) {
          return methodNotAllowed();
        }
        return { status: 200, body: getMetaHealth() };
      }

      if (input.path === '/__meta/config') {
        if (!methodIs(input, 'GET')) {
          return methodNotAllowed();
        }
        return { status: 200, body: getMetaConfig(this.config) };
      }

      if (input.path === '/__meta/log') {
        if (!methodIs(input, 'GET')) {
          return methodNotAllowed();
        }
        return { status: 200, body: getMetaLog() };
      }

      if (input.path === '/__meta/state') {
        if (!methodIs(input, 'GET')) {
          return methodNotAllowed();
        }
        return { status: 200, body: getMetaState() };
      }

      if (input.path === '/__meta/reset') {
        if (!methodIs(input, 'POST')) {
          return methodNotAllowed();
        }
        return { status: 200, body: resetMetaState() };
      }

      if (input.path === '/__meta/commit') {
        if (!methodIs(input, 'POST')) {
          return methodNotAllowed();
        }
        return {
          status: 200,
          body: await commitMetaState(this.config, {
            path: input.path,
            request: {
              headers: input.headers ?? {},
            },
          }),
        };
      }

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
    return this.withRuntimeContext(() => getMetaConfig(this.config));
  }

  getLog(): DraftProxyLogResponse {
    return this.withRuntimeContext(() => getMetaLog());
  }

  getState(): DraftProxyStateResponse {
    return this.withRuntimeContext(() => getMetaState());
  }

  reset(): MetaResetResponse {
    return this.withRuntimeContext(() => resetMetaState());
  }

  clear(): MetaResetResponse {
    return this.reset();
  }

  commit(headers: Record<string, DraftProxyHeaderValue> = {}): Promise<MetaCommitResponse> {
    return this.withRuntimeContext(() =>
      commitMetaState(this.config, {
        path: '/__meta/commit',
        request: { headers },
      }),
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
