import type { AppConfig } from './config.js';
import { commitMetaState, renderMetaWebUi, type MetaCommitResponse } from './meta/routes.js';
import { processProxyGraphQLRequest, type ProxyGraphQLResponse } from './proxy/routes.js';
import { loadNormalizedStateSnapshot } from './state/snapshot-loader.js';
import { InMemoryStore } from './state/store.js';
import { SyntheticIdentityRegistry } from './state/synthetic-identity.js';

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

export interface DraftProxyOptions {
  store?: InMemoryStore;
  syntheticIdentity?: SyntheticIdentityRegistry;
}

export interface DraftProxyConfigSnapshot {
  runtime: {
    readMode: AppConfig['readMode'];
  };
  proxy: {
    port: AppConfig['port'];
    shopifyAdminOrigin: AppConfig['shopifyAdminOrigin'];
  };
  snapshot: {
    enabled: boolean;
    path: string | null;
  };
}

export type DraftProxyLogSnapshot = ReturnType<InMemoryStore['getMetaLog']>;
export type DraftProxyStateSnapshot = ReturnType<InMemoryStore['getState']>;

export interface DraftProxyCommitResult {
  stopIndex: null;
  attempts: MetaCommitResponse['attempts'];
}

export class DraftProxyCommitError extends Error {
  readonly result: MetaCommitResponse;

  constructor(result: MetaCommitResponse) {
    super('DraftProxy commit failed before all staged mutations were replayed.');
    this.name = 'DraftProxyCommitError';
    this.result = result;
  }
}

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

function methodNotAllowed(): DraftProxyHttpResponse {
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

type DraftProxyRouteParams = Record<string, string | undefined>;

interface DraftProxyRoute {
  methods: string[];
  match(path: string): DraftProxyRouteParams | null;
  handle(
    input: DraftProxyRequest,
    params: DraftProxyRouteParams,
  ): DraftProxyHttpResponse | Promise<DraftProxyHttpResponse>;
}

function exactPath(expectedPath: string): (path: string) => DraftProxyRouteParams | null {
  return (path) => (path === expectedPath ? {} : null);
}

function regexPath(pattern: RegExp, ...paramNames: string[]): (path: string) => DraftProxyRouteParams | null {
  return (path) => {
    const match = pattern.exec(path);
    if (!match) {
      return null;
    }

    return Object.fromEntries(paramNames.map((name, index) => [name, match[index + 1]]));
  };
}

export class DraftProxy {
  /** Immutable runtime configuration used for upstream Shopify requests and proxy behavior. */
  readonly config: AppConfig;

  private readonly runtimeStore: InMemoryStore;
  private readonly syntheticIdentity: SyntheticIdentityRegistry;

  /**
   * Creates an isolated draft proxy runtime.
   *
   * Each instance owns its in-memory store and synthetic identity registry unless
   * explicit test/runtime dependencies are supplied.
   */
  constructor(config: AppConfig, options: DraftProxyOptions = {}) {
    this.config = config;
    this.runtimeStore = options.store ?? new InMemoryStore();
    this.syntheticIdentity = options.syntheticIdentity ?? new SyntheticIdentityRegistry();

    if (config.snapshotPath) {
      this.runtimeStore.installSnapshot(loadNormalizedStateSnapshot(config.snapshotPath));
    }
  }

  /**
   * Processes a full HTTP-shaped request without requiring a Koa server.
   *
   * This is the primary embedding API for workers, tests, and other virtualized
   * runtimes that need Shopify-like routing and meta endpoints without opening a
   * listening socket.
   */
  async processRequest(input: DraftProxyRequest): Promise<DraftProxyHttpResponse> {
    for (const route of this.requestRoutes()) {
      const params = route.match(input.path);
      if (!params) {
        continue;
      }

      if (!route.methods.some((method) => methodIs(input, method))) {
        return methodNotAllowed();
      }

      return route.handle(input, params);
    }

    return this.notFound();
  }

  private requestRoutes(): DraftProxyRoute[] {
    return [
      {
        methods: ['GET'],
        match: exactPath('/__meta'),
        handle: () => ({
          status: 200,
          headers: { 'content-type': 'text/html; charset=utf-8' },
          body: renderMetaWebUi(this.config, this.runtimeStore),
        }),
      },
      {
        methods: ['GET'],
        match: exactPath('/__meta/health'),
        handle: () => ({
          status: 200,
          body: {
            ok: true,
            message: 'shopify-draft-proxy is running',
          },
        }),
      },
      {
        methods: ['GET'],
        match: exactPath('/__meta/config'),
        handle: () => ({ status: 200, body: this.getConfig() }),
      },
      {
        methods: ['GET'],
        match: exactPath('/__meta/log'),
        handle: () => ({ status: 200, body: this.getLog() }),
      },
      {
        methods: ['GET'],
        match: exactPath('/__meta/state'),
        handle: () => ({ status: 200, body: this.getState() }),
      },
      {
        methods: ['GET'],
        match: regexPath(META_BULK_OPERATION_RESULT_ROUTE_PATTERN, 'operationId'),
        handle: (_input, params) => this.metaBulkOperationResultResponse(params['operationId']),
      },
      {
        methods: ['POST'],
        match: exactPath('/__meta/reset'),
        handle: () => this.resetResponse(),
      },
      {
        methods: ['POST'],
        match: exactPath('/__meta/commit'),
        handle: (input) => this.commitResponse(input.headers ?? {}),
      },
      {
        methods: ['GET'],
        match: regexPath(BULK_OPERATION_RESULT_ROUTE_PATTERN, 'numericId'),
        handle: (_input, params) => this.bulkOperationResultResponse(params['numericId']),
      },
      {
        methods: ['POST', 'PUT'],
        match: regexPath(STAGED_UPLOAD_ROUTE_PATTERN, 'targetId', 'filename'),
        handle: (input, params) => this.stagedUploadResponse(params['targetId'], params['filename'], input.body),
      },
      {
        methods: ['POST'],
        match: regexPath(ADMIN_GRAPHQL_ROUTE_PATTERN),
        handle: (input) => this.graphqlResponse(input),
      },
    ];
  }

  private notFound(): DraftProxyHttpResponse {
    return {
      status: 404,
      body: { errors: [{ message: 'Not found' }] },
    };
  }

  private metaBulkOperationResultResponse(encodedOperationId: string | undefined): DraftProxyHttpResponse {
    if (!encodedOperationId) {
      return this.bulkOperationResultNotFound();
    }

    const operation = this.runtimeStore.getEffectiveBulkOperationById(decodeURIComponent(encodedOperationId));
    if (!operation?.resultJsonl) {
      return this.bulkOperationResultNotFound();
    }

    return {
      status: 200,
      headers: { 'content-type': 'application/jsonl; charset=utf-8' },
      body: operation.resultJsonl,
    };
  }

  private bulkOperationResultResponse(numericId: string | undefined): DraftProxyHttpResponse {
    const jsonl = numericId
      ? this.runtimeStore.getEffectiveBulkOperationResultJsonl(`gid://shopify/BulkOperation/${numericId}`)
      : null;

    if (jsonl === null) {
      return this.bulkOperationResultNotFound();
    }

    return {
      status: 200,
      headers: { 'content-type': 'application/jsonl; charset=utf-8' },
      body: jsonl,
    };
  }

  private bulkOperationResultNotFound(): DraftProxyHttpResponse {
    return {
      status: 404,
      body: 'Bulk operation result not found',
    };
  }

  private resetResponse(): DraftProxyHttpResponse {
    this.reset();
    return {
      status: 200,
      body: { ok: true, message: 'state reset' },
    };
  }

  private async commitResponse(headers: Record<string, DraftProxyHeaderValue>): Promise<DraftProxyHttpResponse> {
    try {
      const result = await this.commit(headers);
      return {
        status: 200,
        body: { ok: true, ...result },
      };
    } catch (error) {
      if (error instanceof DraftProxyCommitError) {
        return {
          status: 200,
          body: error.result,
        };
      }
      throw error;
    }
  }

  private stagedUploadResponse(
    encodedTargetId: string | undefined,
    encodedFilename: string | undefined,
    body: unknown,
  ): DraftProxyHttpResponse {
    if (!encodedTargetId || !encodedFilename) {
      return this.notFound();
    }

    return {
      status: 201,
      body: this.runtimeStore.stageStagedUpload(
        decodeURIComponent(encodedTargetId),
        decodeURIComponent(encodedFilename),
        requestBodyToText(body),
      ),
    };
  }

  private graphqlResponse(input: DraftProxyRequest): Promise<ProxyGraphQLResponse> {
    return processProxyGraphQLRequest(
      this.config,
      withOptionalHeaders(
        {
          path: input.path,
          body: input.body,
        },
        input.headers,
      ),
      { store: this.runtimeStore, syntheticIdentity: this.syntheticIdentity },
    );
  }

  /** Processes a Shopify Admin GraphQL request through the same runtime path as the HTTP API. */
  processGraphQLRequest(body: unknown, options: DraftProxyGraphQLRequestOptions = {}): Promise<DraftProxyHttpResponse> {
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

  /** Returns the sanitized runtime configuration exposed by `GET /__meta/config`. */
  getConfig(): DraftProxyConfigSnapshot {
    return {
      runtime: {
        readMode: this.config.readMode,
      },
      proxy: {
        port: this.config.port,
        shopifyAdminOrigin: this.config.shopifyAdminOrigin,
      },
      snapshot: {
        enabled: Boolean(this.config.snapshotPath),
        path: this.config.snapshotPath ?? null,
      },
    };
  }

  /** Returns the mutation log in original replay order. */
  getLog(): DraftProxyLogSnapshot {
    return this.runtimeStore.getMetaLog();
  }

  /** Returns the current base and staged in-memory state snapshot. */
  getState(): DraftProxyStateSnapshot {
    return this.runtimeStore.getState();
  }

  /** Clears staged state, mutation log, and synthetic identity counters for this instance. */
  reset(): void {
    this.runtimeStore.resetRuntimeState(this.syntheticIdentity);
  }

  /** Replays staged raw mutations to upstream Shopify in original order. */
  async commit(headers: Record<string, DraftProxyHeaderValue> = {}): Promise<DraftProxyCommitResult> {
    const result = await commitMetaState(
      this.config,
      {
        path: '/__meta/commit',
        request: { headers },
      },
      this.runtimeStore,
    );

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
