import type Koa from 'koa';
import type { UpstreamGraphQLClient } from './upstream-client.js';

const PROXY_USER_AGENT = 'shopify-draft-proxy';

const OMITTED_FORWARD_HEADERS = new Set([
  'connection',
  'content-length',
  'host',
  'keep-alive',
  'proxy-authenticate',
  'proxy-authorization',
  'te',
  'trailer',
  'transfer-encoding',
  'upgrade',
]);

export interface UpstreamGraphQLProxyRequest {
  path?: string;
  body: unknown;
}

export function buildShopifyDraftProxyUserAgent(incomingUserAgent: string | undefined): string {
  const trimmedIncomingUserAgent = incomingUserAgent?.trim();
  if (!trimmedIncomingUserAgent) {
    return PROXY_USER_AGENT;
  }

  return `${PROXY_USER_AGENT} (wrapping ${trimmedIncomingUserAgent})`;
}

export function buildForwardedGraphQLHeaders(ctx: Koa.Context): Record<string, string> {
  const headers: Record<string, string> = {};

  for (const [name, value] of Object.entries(ctx.request.headers)) {
    const normalizedName = name.toLowerCase();
    if (value === undefined || OMITTED_FORWARD_HEADERS.has(normalizedName)) {
      continue;
    }

    headers[normalizedName] = Array.isArray(value) ? value.join(', ') : value;
  }

  headers['content-type'] = 'application/json';
  headers['user-agent'] = buildShopifyDraftProxyUserAgent(headers['user-agent']);

  return headers;
}

export async function requestUpstreamGraphQL(
  upstream: UpstreamGraphQLClient,
  ctx: Koa.Context,
  input: UpstreamGraphQLProxyRequest,
): Promise<Response> {
  return upstream.request({
    path: input.path ?? ctx.path,
    headers: buildForwardedGraphQLHeaders(ctx),
    body: input.body,
  });
}
