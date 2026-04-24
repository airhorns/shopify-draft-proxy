export type ConformanceGraphqlFetch = typeof fetch;

export type AdminGraphqlOptions = {
  adminOrigin: string;
  apiVersion: string;
  headers: Record<string, string>;
  fetchImpl?: ConformanceGraphqlFetch;
};

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

export async function runAdminGraphql(
  options: AdminGraphqlOptions,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<unknown> {
  const fetchImpl = options.fetchImpl ?? fetch;
  const response = await fetchImpl(`${options.adminOrigin}/admin/api/${options.apiVersion}/graphql.json`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...options.headers,
    },
    body: JSON.stringify({ query, variables }),
  });
  const payload = (await response.json()) as unknown;

  if (!response.ok || (typeof payload === 'object' && payload !== null && 'errors' in payload)) {
    throw new Error(formatGraphqlError(payload, response.status));
  }

  return payload;
}
