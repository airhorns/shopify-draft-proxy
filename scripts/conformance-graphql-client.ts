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
  const payload = (await response.json()) as ConformanceGraphqlPayload<TData>;

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
