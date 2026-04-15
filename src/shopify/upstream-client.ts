export interface UpstreamGraphQLRequest {
  path: string;
  headers: Record<string, string>;
  body: unknown;
}

export interface UpstreamGraphQLClient {
  request(input: UpstreamGraphQLRequest): Promise<Response>;
}

export function createUpstreamGraphQLClient(shopifyAdminOrigin: string): UpstreamGraphQLClient {
  return {
    async request(input: UpstreamGraphQLRequest): Promise<Response> {
      const url = new URL(input.path, shopifyAdminOrigin);
      return fetch(url, {
        method: 'POST',
        headers: input.headers,
        body: JSON.stringify(input.body),
      });
    },
  };
}
