import { parse } from 'graphql';

export type ApiSurface = 'admin' | 'storefront';

export type RecordedUpstreamCall = {
  method?: string;
  path?: string;
  apiSurface?: ApiSurface;
  apiVersion?: string;
  endpoint?: string;
  headers?: Record<string, string>;
  operationName?: string;
  variables?: unknown;
  query?: string;
  response?: { status?: number; body?: unknown };
};

export type OutgoingGraphqlRequest = {
  method: string;
  path: string;
  body: string;
  apiSurface?: ApiSurface;
};

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function stableJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map((entry) => stableJson(entry)).join(',')}]`;
  if (isPlainObject(value))
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${stableJson(value[key])}`)
      .join(',')}}`;
  return JSON.stringify(value);
}

export function isGraphqlDocumentText(value: unknown): value is string {
  if (typeof value !== 'string' || value.length === 0) return false;
  try {
    const document = parse(value);
    return document.definitions.length > 0;
  } catch {
    return false;
  }
}

export function apiSurfaceFromGraphqlPath(path: string): ApiSurface | null {
  if (/^\/admin\/api\/[^/]+\/graphql\.json$/u.test(path)) return 'admin';
  if (/^\/api\/[^/]+\/graphql\.json$/u.test(path)) return 'storefront';
  return null;
}

export function graphqlPathForApiSurface(apiSurface: ApiSurface, apiVersion: string): string {
  return apiSurface === 'storefront' ? `/api/${apiVersion}/graphql.json` : `/admin/api/${apiVersion}/graphql.json`;
}

function normalizeMethod(method: string | undefined): string | null {
  return typeof method === 'string' && method.length > 0 ? method.toUpperCase() : null;
}

function requestApiSurface(request: OutgoingGraphqlRequest): ApiSurface | null {
  return request.apiSurface ?? apiSurfaceFromGraphqlPath(request.path);
}

function explicitCallApiSurface(call: RecordedUpstreamCall): ApiSurface | null {
  return call.apiSurface ?? (typeof call.path === 'string' ? apiSurfaceFromGraphqlPath(call.path) : null);
}

function bodyMatchesCall(call: RecordedUpstreamCall, body: string): boolean {
  try {
    const parsed = JSON.parse(body) as Record<string, unknown>;
    // Normalize trailing whitespace so .graphql files with a trailing newline
    // match cassette entries captured without one (and vice versa).
    const outgoingQuery = typeof parsed['query'] === 'string' ? parsed['query'].trimEnd() : null;
    const recordedQuery = typeof call.query === 'string' ? call.query.trimEnd() : null;
    return (
      outgoingQuery !== null &&
      recordedQuery !== null &&
      outgoingQuery === recordedQuery &&
      stableJson(parsed['variables'] ?? {}) === stableJson(call.variables ?? {})
    );
  } catch {
    return false;
  }
}

export function recordedCallMatchesRequest(call: RecordedUpstreamCall, request: OutgoingGraphqlRequest): boolean {
  if (!bodyMatchesCall(call, request.body)) return false;

  const requestSurface = requestApiSurface(request);
  const callSurface = explicitCallApiSurface(call);
  if (requestSurface === null) return false;
  if (callSurface !== null && callSurface !== requestSurface) return false;
  if (callSurface === null && requestSurface === 'storefront') return false;

  const callMethod = normalizeMethod(call.method);
  if (callMethod !== null && callMethod !== normalizeMethod(request.method)) return false;
  if (typeof call.path === 'string' && call.path !== request.path) return false;

  return true;
}

export function recordedCallMatchesBody(call: RecordedUpstreamCall, body: string): boolean {
  const apiSurface = explicitCallApiSurface(call) ?? 'admin';
  const path =
    typeof call.path === 'string'
      ? call.path
      : graphqlPathForApiSurface(apiSurface, typeof call.apiVersion === 'string' ? call.apiVersion : '2026-04');
  return recordedCallMatchesRequest(call, {
    method: call.method ?? 'POST',
    path,
    apiSurface,
    body,
  });
}

export function validateRecordedUpstreamCalls(calls: RecordedUpstreamCall[]): string[] {
  const errors: string[] = [];
  for (let index = 0; index < calls.length; index += 1) {
    const call = calls[index];
    if (!call || typeof call.query !== 'string') {
      errors.push(`upstreamCalls[${index}].query is missing or is not a string`);
      continue;
    }
    if (!isGraphqlDocumentText(call.query)) {
      errors.push(
        `upstreamCalls[${index}].query is not a valid GraphQL document: ${JSON.stringify(call.query).slice(0, 500)}`,
      );
    }
    if (call.apiSurface !== undefined && call.apiSurface !== 'admin' && call.apiSurface !== 'storefront') {
      errors.push(`upstreamCalls[${index}].apiSurface must be "admin" or "storefront"`);
    }
    if (call.method !== undefined && normalizeMethod(call.method) === null) {
      errors.push(`upstreamCalls[${index}].method must be a non-empty string`);
    }
    if (call.path !== undefined) {
      if (typeof call.path !== 'string' || !call.path.startsWith('/')) {
        errors.push(`upstreamCalls[${index}].path must be an absolute request path`);
      } else {
        const pathSurface = apiSurfaceFromGraphqlPath(call.path);
        if (pathSurface === null) {
          errors.push(`upstreamCalls[${index}].path is not a recognized Admin or Storefront GraphQL path`);
        } else if (call.apiSurface !== undefined && call.apiSurface !== pathSurface) {
          errors.push(
            `upstreamCalls[${index}].apiSurface (${call.apiSurface}) does not match path ${call.path} (${pathSurface})`,
          );
        }
      }
    }
    if (call.apiSurface === 'storefront') {
      if (normalizeMethod(call.method) !== 'POST') {
        errors.push(`upstreamCalls[${index}].method must be POST for Storefront GraphQL calls`);
      }
      if (typeof call.path !== 'string') {
        errors.push(`upstreamCalls[${index}].path is required for Storefront GraphQL calls`);
      }
      const headers = isPlainObject(call.headers) ? call.headers : {};
      for (const [name, value] of Object.entries(headers)) {
        if (/storefront.*token/iu.test(name) && (typeof value !== 'string' || !value.startsWith('<redacted:'))) {
          errors.push(`upstreamCalls[${index}].headers.${name} must redact Storefront token values`);
        }
      }
    }
  }
  return errors;
}

function parsedRequestBody(body: string): Record<string, unknown> {
  try {
    return JSON.parse(body) as Record<string, unknown>;
  } catch {
    return {};
  }
}

function truncate(value: string, max = 4000): string {
  return value.length <= max ? value : `${value.slice(0, max)}…<truncated ${value.length - max} chars>`;
}

export function formatRecordedCallMismatch(
  request: OutgoingGraphqlRequest,
  calls: RecordedUpstreamCall[],
  consumedCalls: ReadonlySet<number>,
): string {
  const parsed = parsedRequestBody(request.body);
  const requestQuery = typeof parsed['query'] === 'string' ? parsed['query'] : '<missing query>';
  const requestVariables = stableJson(parsed['variables'] ?? {});
  const surface = requestApiSurface(request) ?? '<unrecognized>';
  const candidates = calls.map((call, index) => ({ call, index })).filter(({ index }) => !consumedCalls.has(index));
  const candidateSummary = candidates
    .slice(0, 12)
    .map(({ call, index }) => {
      const variables = stableJson(call.variables ?? {});
      const query = typeof call.query === 'string' ? truncate(call.query, 1200) : '<missing query>';
      const candidateSurface = explicitCallApiSurface(call) ?? 'admin (legacy default)';
      return [
        `candidate upstreamCalls[${index}]`,
        `  method: ${call.method ?? '<legacy: POST not recorded>'}`,
        `  apiSurface: ${candidateSurface}`,
        `  path: ${call.path ?? '<legacy: path not recorded>'}`,
        `  operationName: ${call.operationName ?? '<missing>'}`,
        `  variables: ${variables}`,
        `  query: ${query}`,
      ].join('\n');
    })
    .join('\n');

  const omitted = candidates.length > 12 ? `\n... ${candidates.length - 12} more unconsumed candidate(s) omitted` : '';
  return [
    'No exact recorded upstream call matched the proxy request.',
    'Cassette matching is strict: method, API surface/path, recorded query text, and variables must exactly equal the outgoing request.',
    `Outgoing method: ${normalizeMethod(request.method) ?? '<missing>'}`,
    `Outgoing apiSurface: ${surface}`,
    `Outgoing path: ${request.path}`,
    `Outgoing variables: ${requestVariables}`,
    'Outgoing query:',
    truncate(requestQuery),
    candidateSummary
      ? `Unconsumed recorded upstream candidates:\n${candidateSummary}${omitted}`
      : 'No unconsumed recorded upstream candidates remain.',
  ].join('\n');
}
