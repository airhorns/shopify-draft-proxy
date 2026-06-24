export type ConformanceGraphqlFetch = typeof fetch;

export type AdminGraphqlOptions = {
  adminOrigin: string;
  apiVersion: string;
  headers: Record<string, string>;
  fetchImpl?: ConformanceGraphqlFetch;
};

export type ConformanceGraphqlPayload<TData = unknown> = {
  data?: TData;
  errors?: unknown;
  extensions?: unknown;
};

export type ConformanceGraphqlResult<TData = unknown> = {
  status: number;
  payload: ConformanceGraphqlPayload<TData>;
};

export type AdminGraphqlClient = {
  runGraphql: <TData = unknown>(
    query: string,
    variables?: Record<string, unknown>,
  ) => Promise<ConformanceGraphqlPayload<TData>>;
  runGraphqlRequest: <TData = unknown>(
    query: string,
    variables?: Record<string, unknown>,
  ) => Promise<ConformanceGraphqlResult<TData>>;
  runGraphqlRaw: <TData = unknown>(
    query: string,
    variables?: Record<string, unknown>,
  ) => Promise<ConformanceGraphqlResult<TData>>;
};

export class ConformanceGraphqlError<TData = unknown> extends Error {
  readonly result: ConformanceGraphqlResult<TData>;

  constructor(result: ConformanceGraphqlResult<TData>) {
    super(formatGraphqlError(result.payload, result.status));
    this.name = 'ConformanceGraphqlError';
    this.result = result;
  }
}

export function formatGraphqlError(payload: unknown, status: number): string {
  if (typeof payload === 'object' && payload !== null && 'errors' in payload) {
    const errors = (payload as { errors?: unknown }).errors;
    if (Array.isArray(errors)) {
      const messages = errors
        .map((error) => {
          if (typeof error === 'object' && error !== null && 'message' in error) {
            const message = (error as { message?: unknown }).message;
            return typeof message === 'string' ? message : null;
          }
          return null;
        })
        .filter((message): message is string => message !== null);
      if (messages.length > 0) {
        return messages.join('; ');
      }
    }

    if (typeof errors === 'string') {
      return errors;
    }
  }

  return `HTTP ${status}`;
}

async function readGraphqlPayload<TData>(response: Response): Promise<ConformanceGraphqlPayload<TData>> {
  const contentType = response.headers.get('content-type') ?? '';
  if (contentType.includes('application/json')) {
    return (await response.json()) as ConformanceGraphqlPayload<TData>;
  }

  return {
    errors: await response.text(),
  };
}

export async function runAdminGraphqlRequest<TData = unknown>(
  options: AdminGraphqlOptions,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<ConformanceGraphqlResult<TData>> {
  const fetchImpl = options.fetchImpl ?? fetch;
  const response = await fetchImpl(`${options.adminOrigin}/admin/api/${options.apiVersion}/graphql.json`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...options.headers,
    },
    body: JSON.stringify({ query, variables }),
  });
  const payload = await readGraphqlPayload<TData>(response);

  return {
    status: response.status,
    payload,
  };
}

export async function runAdminGraphql<TData = unknown>(
  options: AdminGraphqlOptions,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<ConformanceGraphqlPayload<TData>> {
  const result = await runAdminGraphqlRequest<TData>(options, query, variables);

  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new ConformanceGraphqlError(result);
  }

  return result.payload;
}

export function createAdminGraphqlClient(options: AdminGraphqlOptions): AdminGraphqlClient {
  const runGraphqlRequest = <TData = unknown>(query: string, variables: Record<string, unknown> = {}) =>
    runAdminGraphqlRequest<TData>(options, query, variables);

  return {
    runGraphql: <TData = unknown>(query: string, variables: Record<string, unknown> = {}) =>
      runAdminGraphql<TData>(options, query, variables),
    runGraphqlRequest,
    runGraphqlRaw: runGraphqlRequest,
  };
}

export type StorefrontGraphqlOptions = {
  /** Full store URL, e.g. https://my-store.myshopify.com */
  storeOrigin: string;
  apiVersion: string;
  /** Storefront access token — sent as X-Shopify-Storefront-Access-Token */
  storefrontAccessToken: string;
  fetchImpl?: ConformanceGraphqlFetch;
};

export async function runStorefrontGraphqlRequest<TData = unknown>(
  options: StorefrontGraphqlOptions,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<ConformanceGraphqlResult<TData>> {
  const fetchImpl = options.fetchImpl ?? fetch;
  const endpoint = `${options.storeOrigin}/api/${options.apiVersion}/graphql.json`;
  const response = await fetchImpl(endpoint, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Shopify-Storefront-Access-Token': options.storefrontAccessToken,
    },
    body: JSON.stringify({ query, variables }),
  });
  const payload = await readGraphqlPayload<TData>(response);
  return { status: response.status, payload };
}

export async function runStorefrontGraphql<TData = unknown>(
  options: StorefrontGraphqlOptions,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<ConformanceGraphqlPayload<TData>> {
  const result = await runStorefrontGraphqlRequest<TData>(options, query, variables);

  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new ConformanceGraphqlError(result);
  }

  return result.payload;
}

export function createStorefrontGraphqlClient(options: StorefrontGraphqlOptions) {
  return {
    runGraphqlRequest: <TData = unknown>(query: string, variables: Record<string, unknown> = {}) =>
      runStorefrontGraphqlRequest<TData>(options, query, variables),
    runGraphql: <TData = unknown>(query: string, variables: Record<string, unknown> = {}) =>
      runStorefrontGraphql<TData>(options, query, variables),
  };
}
